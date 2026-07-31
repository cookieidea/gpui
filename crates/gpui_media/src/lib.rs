//! Reusable GPUI video playback component.
//!
//! A pluggable [`MediaBackend`] owns demuxing, video/audio decoding, audio
//! output and the shared playback clock. [`VideoPlayer`] renders only the
//! newest decoded video frame with GPUI's dynamic `surface` element. It
//! deliberately contains no controls, pointer behavior, status overlay or
//! fullscreen policy; host applications build those features from the
//! exported state and events.

mod backends;
mod container;
mod error;
mod frame;
mod frame_extractor;
mod media_backend;
mod media_info;
mod player;
mod source;
mod stats;
mod subtitles;
mod timeline;
mod window_sizing;

#[cfg(feature = "backend-decv")]
pub use backends::decv::{DecvBackend, DecvBackendOptions, DecvParallelism};
#[cfg(feature = "backend-system")]
pub use backends::system::SystemBackend;
pub use container::{VideoContainer, video_container};
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
pub use media_info::{
    AudioStreamInfo, MediaInfo, MediaStreamId, SubtitleStreamInfo, VideoStreamInfo,
};
pub use player::{
    PlaybackState, VideoPlayer, VideoPlayerBuilder, VideoPlayerEvent, VideoPlayerOptions,
};
pub use source::{MediaSource, NetworkSourceOptions};
pub use stats::VideoPlaybackStats;
pub use subtitles::{
    ParsedSubtitles, SubtitleCue, SubtitleEvent, SubtitleFormat, detect_subtitle_format,
    parse_subtitles,
};
pub use timeline::{PlaybackTimeline, SeekMode};
pub use window_sizing::{fit_video_window_bounds, fit_video_window_size};

/// Initializes the default built-in backend, when it requires initialization.
///
/// Custom backends initialize themselves when opening a session, and this
/// function is a no-op when no built-in backend requiring initialization is
/// enabled.
pub fn init() -> MediaResult<()> {
    #[cfg(feature = "backend-system")]
    {
        return SystemBackend::initialize();
    }

    #[cfg(not(feature = "backend-system"))]
    {
        Ok(())
    }
}
