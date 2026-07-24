//! Reusable GPUI video playback component.
//!
//! A pluggable [`MediaBackend`] owns demuxing, video/audio decoding, audio
//! output and the shared playback clock. [`VideoPlayer`] renders only the
//! newest decoded video frame with GPUI's dynamic `surface` element. It
//! deliberately contains no controls, pointer behavior, status overlay or
//! fullscreen policy; host applications build those features from the
//! exported state and events.

mod container;
#[cfg(feature = "backend-decv")]
mod decv_backend;
mod error;
mod frame;
mod frame_extractor;
#[cfg(feature = "backend-gstreamer")]
mod gstreamer_backend;
mod media_backend;
#[cfg(feature = "backend-gstreamer")]
mod network;
mod player;
mod source;
mod stats;
mod timeline;
mod window_sizing;

pub use container::{VideoContainer, video_container};
#[cfg(feature = "backend-decv")]
pub use decv_backend::{DecvBackend, DecvBackendOptions, DecvParallelism};
pub use error::{MediaError, MediaErrorKind, MediaRecovery, MediaResult};
pub use frame::{FrameTransport, VideoFrame};
pub use frame_extractor::{
    FrameExtractionSuperseded, VideoFrameExtractor, VideoFrameExtractorOptions,
};
pub use media_backend::{
    FrameExtractionSession, FrameExtractorBackendRequest, FrameTransportPreference, MediaBackend,
    MediaBackendEvent, MediaCapabilities, MediaOutputSink, MediaPlaybackRequest,
    MediaPlaybackSession, TransportChange,
};
pub use player::{
    PlaybackState, VideoPlayer, VideoPlayerBuilder, VideoPlayerEvent, VideoPlayerOptions,
};
pub use source::{MediaSource, NetworkSourceOptions};
pub use stats::VideoPlaybackStats;
pub use timeline::{PlaybackTimeline, SeekMode};
pub use window_sizing::{fit_video_window_bounds, fit_video_window_size};

/// Initializes the default built-in backend, when it requires initialization.
///
/// This is retained for compatibility with applications that initialize the
/// default GStreamer backend during startup. Custom backends initialize
/// themselves when opening a session, and this function is a no-op when no
/// built-in backend feature is enabled.
pub fn init() -> anyhow::Result<()> {
    #[cfg(feature = "backend-gstreamer")]
    {
        return gstreamer_backend::initialize();
    }

    #[cfg(not(feature = "backend-gstreamer"))]
    {
        Ok(())
    }
}

#[cfg(feature = "backend-gstreamer")]
pub use gstreamer_backend::GstreamerBackend;
