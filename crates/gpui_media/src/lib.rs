//! Reusable GPUI audio and video playback components.
//!
//! A pluggable `MediaBackend` owns demuxing, video/audio decoding, audio
//! output and the shared playback clock. `VideoPlayer` renders only the
//! newest decoded video frame with GPUI's dynamic `surface` element. It
//! deliberately contains no controls, pointer behavior, status overlay or
//! fullscreen policy; host applications build those features from the
//! exported state and events.
//!
//! `AudioPlayer` is independent from the video backend boundary. It uses
//! Symphonia and CPAL consistently across platforms and exposes no built-in UI.

#[cfg(feature = "audio")]
mod audio;
mod error;
mod playback_state;
mod source;
mod timeline;
#[cfg(feature = "video")]
mod video;

#[cfg(feature = "audio")]
pub use audio::{
    AudioInfo, AudioPlayer, AudioPlayerBuilder, AudioPlayerEvent, AudioPlayerOptions, AudioSource,
    AudioStreamHint, AudioStreamWriter,
};
pub use error::{MediaError, MediaErrorKind, MediaRecovery, MediaResult};
pub use playback_state::PlaybackState;
pub use source::{MediaSource, NetworkSourceOptions};
pub use timeline::{PlaybackTimeline, SeekMode};
#[cfg(feature = "video")]
pub use video::*;

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
