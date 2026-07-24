use std::{sync::Arc, time::Duration};

use anyhow::{Result, bail};
use gpui::GpuSpecs;

use crate::{MediaSource, PlaybackTimeline, SeekMode, VideoFrame, stats::PlaybackCounters};

/// A backend-independent event produced by a playback session.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum BackendEvent {
    Buffering(u8),
    Ended,
    Error(Arc<str>),
}

/// Capabilities exposed by one opened playback session.
///
/// Capabilities belong to the session rather than the factory because support
/// can depend on the selected media streams and source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackendCapabilities {
    pub audio: bool,
    pub seeking: bool,
    pub accurate_seeking: bool,
    pub playback_rate: bool,
    pub frame_stepping: bool,
    pub frame_extraction: bool,
    pub transport_switching: bool,
}

/// Preferred way for decoded frames to reach GPUI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameTransportPreference {
    /// Let the backend choose the best transport for the active renderer.
    #[default]
    Auto,
    /// Prefer a native transport but permit a CPU fallback.
    PreferNative,
    /// Require portable CPU-backed frames.
    CpuOnly,
}

/// Result of asking an opened backend to change its frame transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportChange {
    Unchanged,
    Reconfigured,
}

/// Data supplied when a backend opens a playback session.
#[derive(Clone, Debug)]
pub struct PlaybackBackendRequest {
    pub source: MediaSource,
    pub gpu_specs: Option<GpuSpecs>,
}

/// Data supplied when a backend opens an independent frame extractor.
#[derive(Clone, Debug)]
pub struct FrameExtractorBackendRequest {
    pub source: MediaSource,
    pub timeout: Duration,
}

/// Thread-safe publisher shared with a playback backend.
///
/// Frames use a latest-only bounded queue. Publishing while the queue is full
/// discards the older frame and records that drop in the common player stats.
#[derive(Clone)]
pub struct PlaybackOutputSink {
    frames: async_channel::Sender<Arc<VideoFrame>>,
    frame_drain: async_channel::Receiver<Arc<VideoFrame>>,
    events: async_channel::Sender<BackendEvent>,
    counters: Arc<PlaybackCounters>,
}

pub(crate) struct PlaybackOutput {
    pub(crate) frames: async_channel::Receiver<Arc<VideoFrame>>,
    pub(crate) events: async_channel::Receiver<BackendEvent>,
    pub(crate) counters: Arc<PlaybackCounters>,
}

impl PlaybackOutputSink {
    pub(crate) fn channel() -> (Self, PlaybackOutput) {
        let (frame_tx, frame_rx) = async_channel::bounded(1);
        let (event_tx, event_rx) = async_channel::unbounded();
        let counters = Arc::new(PlaybackCounters::default());

        (
            Self {
                frames: frame_tx,
                frame_drain: frame_rx.clone(),
                events: event_tx,
                counters: counters.clone(),
            },
            PlaybackOutput {
                frames: frame_rx,
                events: event_rx,
                counters,
            },
        )
    }

    /// Publishes a decoded frame, replacing a stale frame that has not yet
    /// reached the GPUI player entity.
    ///
    /// Returns `true` when an older frame was dropped.
    pub fn publish_frame(&self, frame: Arc<VideoFrame>) -> bool {
        self.counters.record_decoded_frame();
        match self.frames.try_send(frame) {
            Ok(()) | Err(async_channel::TrySendError::Closed(_)) => false,
            Err(async_channel::TrySendError::Full(frame)) => {
                let dropped = self.frame_drain.try_recv().is_ok();
                if dropped {
                    self.counters.record_dropped_frame();
                }
                let _ = self.frames.try_send(frame);
                dropped
            }
        }
    }

    /// Publishes a playback event. Returns `false` after the player has closed
    /// its event stream.
    pub fn emit(&self, event: BackendEvent) -> bool {
        self.events.send_blocking(event).is_ok()
    }

    pub fn is_closed(&self) -> bool {
        self.frames.is_closed() && self.events.is_closed()
    }
}

/// One opened, stateful playback session.
///
/// Methods are synchronous control operations. A backend may run demuxing,
/// decoding and audio on its own workers and publish output through the sink
/// passed to [`VideoBackend::open_playback`].
pub trait PlaybackSession: Send {
    fn capabilities(&self) -> BackendCapabilities;

    fn play(&mut self) -> Result<()>;
    fn pause(&mut self) -> Result<()>;
    fn timeline(&self) -> PlaybackTimeline;

    fn reload(&mut self, _autoplay: bool) -> Result<()> {
        bail!("this video backend cannot reload its active source")
    }

    fn seek_to(&mut self, _position: Duration, _mode: SeekMode) -> Result<()> {
        bail!("this video backend does not support seeking")
    }

    fn step_forward(&mut self, _frames: u64) -> Result<()> {
        bail!("this video backend does not support frame stepping")
    }

    fn set_playback_rate(&mut self, _rate: f64) -> Result<()> {
        bail!("this video backend does not support playback-rate changes")
    }

    fn set_volume(&mut self, _volume: f64) {}
    fn set_muted(&mut self, _muted: bool) {}

    fn restart(&mut self) -> Result<()> {
        self.seek_to(Duration::ZERO, SeekMode::KeyFrame)?;
        self.play()
    }

    fn set_frame_transport_preference(
        &mut self,
        _preference: FrameTransportPreference,
    ) -> Result<TransportChange> {
        bail!("this video backend cannot change frame transport")
    }
}

/// Blocking frame extraction primitive owned by the generic extractor worker.
pub trait FrameExtractionSession: Send {
    fn initial_frame(&mut self) -> Result<Arc<VideoFrame>>;
    fn frame_at(&mut self, position: Duration, seek_mode: SeekMode) -> Result<Arc<VideoFrame>>;
}

/// Factory for playback and frame-extraction sessions.
///
/// Applications may implement this trait in another crate and pass it to
/// `VideoPlayer` or `VideoFrameExtractor` without enabling a built-in backend.
pub trait VideoBackend: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    fn open_playback(
        &self,
        request: PlaybackBackendRequest,
        output: PlaybackOutputSink,
    ) -> Result<Box<dyn PlaybackSession>>;

    fn open_frame_extractor(
        &self,
        _request: FrameExtractorBackendRequest,
    ) -> Result<Box<dyn FrameExtractionSession>> {
        bail!(
            "video backend {} does not support frame extraction",
            self.name()
        )
    }
}

pub(crate) fn default_backend() -> Result<Arc<dyn VideoBackend>> {
    #[cfg(feature = "backend-gstreamer")]
    {
        return Ok(Arc::new(crate::gstreamer_backend::GstreamerBackend));
    }

    #[cfg(all(not(feature = "backend-gstreamer"), feature = "backend-decv"))]
    {
        return Ok(Arc::new(crate::decv_backend::DecvBackend));
    }

    #[cfg(not(any(feature = "backend-gstreamer", feature = "backend-decv")))]
    {
        bail!(
            "gpui_video has no built-in backend; enable `backend-gstreamer` or `backend-decv`, or pass a custom backend"
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::{DevicePixels, SurfaceFrame, SurfaceHandle, size};

    use super::PlaybackOutputSink;
    use crate::VideoFrame;

    fn frame(sequence: u64) -> Arc<VideoFrame> {
        let surface = SurfaceFrame::rgba(
            SurfaceHandle::new(),
            sequence,
            size(DevicePixels(1), DevicePixels(1)),
            vec![0, 0, 0, 255],
            4,
        )
        .unwrap();
        Arc::new(VideoFrame::new(Arc::new(surface), None, None))
    }

    #[test]
    fn output_sink_keeps_only_the_latest_frame() {
        let (sink, output) = PlaybackOutputSink::channel();

        assert!(!sink.publish_frame(frame(1)));
        assert!(sink.publish_frame(frame(2)));
        assert_eq!(output.frames.try_recv().unwrap().surface().sequence(), 2);

        let stats = output.counters.snapshot(0);
        assert_eq!(stats.decoded_frames(), 2);
        assert_eq!(stats.dropped_frames(), 1);
    }
}
