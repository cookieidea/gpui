use crate::{
    FrameExtractionSession, FrameExtractorBackendRequest, MediaBackend, MediaOutputSink,
    MediaPlaybackRequest, MediaPlaybackSession, MediaResult,
};

mod gstreamer;

use gstreamer::LinuxSystemBackend;

pub(super) fn initialize() -> MediaResult<()> {
    gstreamer::initialize()
}

pub(super) fn open_playback(
    request: MediaPlaybackRequest,
    output: MediaOutputSink,
) -> MediaResult<Box<dyn MediaPlaybackSession>> {
    LinuxSystemBackend.open_playback(request, output)
}

pub(super) fn open_frame_extractor(
    request: FrameExtractorBackendRequest,
) -> MediaResult<Box<dyn FrameExtractionSession>> {
    LinuxSystemBackend.open_frame_extractor(request)
}
