//! Platform-selected system media backend.

use crate::{
    FrameExtractionSession, FrameExtractorBackendRequest, MediaBackend, MediaOutputSink,
    MediaPlaybackRequest, MediaPlaybackSession, MediaResult,
};

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod gstreamer;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
use unsupported as platform;
#[cfg(target_os = "windows")]
use windows as platform;

/// The default backend, implemented by the selected system media stack for the
/// current operating system.
///
/// Linux and macOS use the system GStreamer registry, while Windows uses Media
/// Foundation.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemBackend;

impl SystemBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn initialize() -> MediaResult<()> {
        platform::initialize()
    }
}

impl MediaBackend for SystemBackend {
    fn name(&self) -> &'static str {
        "system"
    }

    fn open_playback(
        &self,
        request: MediaPlaybackRequest,
        output: MediaOutputSink,
    ) -> MediaResult<Box<dyn MediaPlaybackSession>> {
        platform::open_playback(request, output)
    }

    fn open_frame_extractor(
        &self,
        request: FrameExtractorBackendRequest,
    ) -> MediaResult<Box<dyn FrameExtractionSession>> {
        platform::open_frame_extractor(request)
    }
}
