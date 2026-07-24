use std::{sync::Arc, time::Duration};

use anyhow::{Result, bail};
use gpui::{DevicePixels, SurfaceFrame, SurfaceHandle, size};
use gpui_video::{
    BackendCapabilities, FrameExtractionSession, FrameExtractorBackendRequest, MediaSource,
    PlaybackBackendRequest, PlaybackOutputSink, PlaybackSession, PlaybackTimeline, SeekMode,
    VideoBackend, VideoFrame, VideoFrameExtractor,
};

#[derive(Clone, Copy)]
struct CustomBackend;

impl VideoBackend for CustomBackend {
    fn name(&self) -> &'static str {
        "custom-test"
    }

    fn open_playback(
        &self,
        _request: PlaybackBackendRequest,
        output: PlaybackOutputSink,
    ) -> Result<Box<dyn PlaybackSession>> {
        output.publish_frame(test_frame(1, Duration::ZERO));
        Ok(Box::new(CustomPlayback))
    }

    fn open_frame_extractor(
        &self,
        _request: FrameExtractorBackendRequest,
    ) -> Result<Box<dyn FrameExtractionSession>> {
        Ok(Box::new(CustomExtractor))
    }
}

struct CustomPlayback;

impl PlaybackSession for CustomPlayback {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            seeking: true,
            frame_extraction: true,
            ..Default::default()
        }
    }

    fn play(&mut self) -> Result<()> {
        Ok(())
    }

    fn pause(&mut self) -> Result<()> {
        Ok(())
    }

    fn reload(&mut self, _autoplay: bool) -> Result<()> {
        Ok(())
    }

    fn timeline(&self) -> PlaybackTimeline {
        PlaybackTimeline::new(Duration::ZERO, Some(Duration::from_secs(10)), true)
    }

    fn seek_to(&mut self, _position: Duration, _mode: SeekMode) -> Result<()> {
        Ok(())
    }

    fn step_forward(&mut self, _frames: u64) -> Result<()> {
        bail!("frame stepping is unsupported")
    }

    fn set_playback_rate(&mut self, _rate: f64) -> Result<()> {
        bail!("playback rate changes are unsupported")
    }
}

struct CustomExtractor;

impl FrameExtractionSession for CustomExtractor {
    fn initial_frame(&mut self) -> Result<Arc<VideoFrame>> {
        Ok(test_frame(1, Duration::ZERO))
    }

    fn frame_at(&mut self, position: Duration, _seek_mode: SeekMode) -> Result<Arc<VideoFrame>> {
        Ok(test_frame(2, position))
    }
}

fn test_frame(sequence: u64, timestamp: Duration) -> Arc<VideoFrame> {
    let surface = SurfaceFrame::rgba(
        SurfaceHandle::new(),
        sequence,
        size(DevicePixels(1), DevicePixels(1)),
        vec![20, 40, 60, 255],
        4,
    )
    .unwrap();
    Arc::new(VideoFrame::new(
        Arc::new(surface),
        Some(timestamp),
        Some(Duration::from_millis(40)),
    ))
}

#[test]
fn external_backend_drives_the_generic_frame_extractor() {
    let source = MediaSource::from_uri("custom-test://video").unwrap();
    let backend: Arc<dyn VideoBackend> = Arc::new(CustomBackend);
    let extractor = VideoFrameExtractor::new_with_backend(source, backend).unwrap();

    let initial = extractor.initial_frame_blocking().unwrap();
    assert_eq!(initial.timestamp(), Some(Duration::ZERO));

    let preview = extractor.frame_at_blocking(Duration::from_secs(3)).unwrap();
    assert_eq!(preview.timestamp(), Some(Duration::from_secs(3)));
    assert_eq!(preview.surface().sequence(), 2);
}
