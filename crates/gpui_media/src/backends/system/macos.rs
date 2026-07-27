use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use core_foundation::base::TCFType as _;
use core_video::pixel_buffer::{
    CVPixelBuffer as NativePixelBuffer, CVPixelBufferRef, kCVPixelFormatType_32BGRA,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
};
use dispatch2::{MainThreadBound, run_on_main};
use gpui::{
    Bounds, ColorRange, CoreVideoHandle, DevicePixels, SurfaceColorInfo, SurfaceFormat,
    SurfaceFrame, SurfaceHandle, YuvMatrix, point, size,
};
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, runtime::AnyObject};
use objc2_av_foundation::{AVPlayer, AVPlayerItem, AVPlayerItemStatus, AVPlayerItemVideoOutput};
use objc2_core_media::CMTime;
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetPlaneCount, CVPixelBufferGetWidth,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use crate::{
    FrameExtractionSession, FrameExtractorBackendRequest, MediaBackendEvent, MediaCapabilities,
    MediaError, MediaErrorKind, MediaOutputSink, MediaPlaybackRequest, MediaPlaybackSession,
    MediaRecovery, MediaResult, PlaybackTimeline, SeekMode, VideoFrame,
};

const MACOS_TIME_SCALE: i32 = 1_000_000;
const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(5);

pub(super) fn initialize() -> MediaResult<()> {
    Ok(())
}

pub(super) fn open_playback(
    request: MediaPlaybackRequest,
    output: MediaOutputSink,
) -> MediaResult<Box<dyn MediaPlaybackSession>> {
    MacPlayback::new(request, output)
        .map(|playback| Box::new(playback) as Box<dyn MediaPlaybackSession>)
}

pub(super) fn open_frame_extractor(
    request: FrameExtractorBackendRequest,
) -> MediaResult<Box<dyn FrameExtractionSession>> {
    MacFrameExtractor::new(request)
        .map(|extractor| Box::new(extractor) as Box<dyn FrameExtractionSession>)
}

struct MacMedia {
    player: objc2::rc::Retained<AVPlayer>,
    item: objc2::rc::Retained<AVPlayerItem>,
    video_output: objc2::rc::Retained<AVPlayerItemVideoOutput>,
    surface_handle: SurfaceHandle,
    sequence: u64,
}

impl MacMedia {
    fn new(uri: &str, mtm: MainThreadMarker) -> MediaResult<Self> {
        let uri = NSString::from_str(uri);
        let url = NSURL::URLWithString(&uri)
            .ok_or_else(|| MediaError::invalid_input("AVFoundation rejected the media URI"))?;

        let item = unsafe { AVPlayerItem::initWithURL(AVPlayerItem::alloc(mtm), &url) };
        let video_output = unsafe {
            AVPlayerItemVideoOutput::initWithPixelBufferAttributes(
                AVPlayerItemVideoOutput::alloc(),
                Some(&pixel_buffer_attributes()),
            )
        };
        unsafe {
            video_output.setSuppressesPlayerRendering(true);
            item.addOutput(&video_output);
        }
        let player = unsafe { AVPlayer::initWithPlayerItem(AVPlayer::alloc(mtm), Some(&item)) };

        Ok(Self {
            player,
            item,
            video_output,
            surface_handle: SurfaceHandle::new(),
            sequence: 1,
        })
    }

    fn timeline(&self) -> PlaybackTimeline {
        let position =
            duration_from_cm_time(unsafe { self.player.currentTime() }).unwrap_or(Duration::ZERO);
        let duration = duration_from_cm_time(unsafe { self.item.duration() });
        PlaybackTimeline::new(position, duration, duration.is_some())
    }

    fn seek(&self, position: Duration, mode: SeekMode) {
        let target = cm_time(position);
        let tolerance = match mode {
            SeekMode::Accurate => cm_time(Duration::ZERO),
            SeekMode::Interactive | SeekMode::KeyFrame => cm_time(Duration::from_millis(500)),
        };
        unsafe {
            self.player
                .seekToTime_toleranceBefore_toleranceAfter(target, tolerance, tolerance);
        }
    }

    fn status_error(&self) -> Option<MediaError> {
        if unsafe { self.item.status() } != AVPlayerItemStatus::Failed {
            return None;
        }
        let message = unsafe { self.item.error() }
            .map(|error| error.localizedDescription().to_string())
            .unwrap_or_else(|| "AVFoundation failed to play the media source".to_owned());
        Some(MediaError::new(
            MediaErrorKind::Decode,
            message,
            MediaRecovery::Reload,
        ))
    }

    fn is_ready(&self) -> bool {
        (unsafe { self.item.status() }) == AVPlayerItemStatus::ReadyToPlay
    }

    fn is_buffering(&self) -> bool {
        unsafe {
            self.item.status() == AVPlayerItemStatus::ReadyToPlay
                && self.item.isPlaybackBufferEmpty()
                && !self.item.isPlaybackLikelyToKeepUp()
        }
    }

    fn is_ended(&self) -> bool {
        let timeline = self.timeline();
        timeline.duration().is_some_and(|duration| {
            !duration.is_zero()
                && timeline.position() >= duration.saturating_sub(Duration::from_millis(5))
                && (unsafe { self.player.rate() }) == 0.0
        })
    }

    fn copy_current_frame(&mut self) -> MediaResult<Option<Arc<VideoFrame>>> {
        let item_time = unsafe { self.player.currentTime() };
        if !unsafe { self.video_output.hasNewPixelBufferForItemTime(item_time) } {
            return Ok(None);
        }

        let mut display_time = item_time;
        let Some(pixel_buffer) = (unsafe {
            self.video_output
                .copyPixelBufferForItemTime_itemTimeForDisplay(item_time, &mut display_time)
        }) else {
            return Ok(None);
        };

        let frame = video_frame_from_pixel_buffer(
            &pixel_buffer,
            self.surface_handle.clone(),
            self.sequence,
            display_time,
        )?;
        self.sequence = self.sequence.wrapping_add(1);
        Ok(Some(Arc::new(frame)))
    }
}

struct MacPlayback {
    media: Arc<MainThreadBound<std::sync::Mutex<MacMedia>>>,
    output: MediaOutputSink,
    pending_ready: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    _poll_thread: JoinHandle<()>,
}

impl MacPlayback {
    fn new(request: MediaPlaybackRequest, output: MediaOutputSink) -> MediaResult<Self> {
        reject_unsupported_network_options(&request)?;
        let uri = request.source.uri().to_owned();
        let media = run_on_main(move |mtm| {
            MacMedia::new(&uri, mtm)
                .map(|media| Arc::new(MainThreadBound::new(std::sync::Mutex::new(media), mtm)))
        })?;

        let pending_ready = Arc::new(AtomicBool::new(true));
        let shutdown = Arc::new(AtomicBool::new(false));
        let poll_thread = start_frame_poll(
            media.clone(),
            output.clone(),
            pending_ready.clone(),
            shutdown.clone(),
        )?;

        Ok(Self {
            media,
            output,
            pending_ready,
            shutdown,
            _poll_thread: poll_thread,
        })
    }

    fn with_media<R: Send>(&self, f: impl Send + FnOnce(&mut MacMedia) -> R) -> R {
        self.media.get_on_main(|media| {
            let mut media = media.lock().expect("AVFoundation media mutex poisoned");
            f(&mut media)
        })
    }
}

impl Drop for MacPlayback {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

impl MediaPlaybackSession for MacPlayback {
    fn capabilities(&self) -> MediaCapabilities {
        MediaCapabilities {
            video: true,
            audio: true,
            seeking: true,
            accurate_seeking: true,
            playback_rate: true,
            frame_stepping: true,
            frame_extraction: true,
            ..MediaCapabilities::default()
        }
    }

    fn play(&mut self) -> MediaResult<()> {
        self.with_media(|media| unsafe { media.player.play() });
        Ok(())
    }

    fn pause(&mut self) -> MediaResult<()> {
        self.with_media(|media| unsafe { media.player.pause() });
        Ok(())
    }

    fn reload(&mut self, autoplay: bool) -> MediaResult<()> {
        self.with_media(move |media| {
            media.seek(Duration::ZERO, SeekMode::KeyFrame);
            unsafe {
                if autoplay {
                    media.player.play();
                } else {
                    media.player.pause();
                }
            }
        });
        self.pending_ready.store(true, Ordering::Release);
        Ok(())
    }

    fn timeline(&self) -> PlaybackTimeline {
        self.with_media(|media| media.timeline())
    }

    fn seek_to(&mut self, position: Duration, mode: SeekMode) -> MediaResult<()> {
        self.with_media(move |media| media.seek(position, mode));
        self.pending_ready.store(true, Ordering::Release);
        Ok(())
    }

    fn step_forward(&mut self, frames: u64) -> MediaResult<()> {
        let frames = isize::try_from(frames)
            .map_err(|_| MediaError::invalid_input("frame step amount is too large"))?;
        self.with_media(move |media| unsafe {
            media.player.pause();
            media.item.stepByCount(frames);
        });
        self.pending_ready.store(true, Ordering::Release);
        Ok(())
    }

    fn set_playback_rate(&mut self, rate: f64) -> MediaResult<()> {
        if !rate.is_finite() || rate <= 0.0 || rate > f32::MAX as f64 {
            return Err(MediaError::invalid_input(
                "playback rate must be finite and greater than zero",
            ));
        }
        self.with_media(move |media| unsafe { media.player.setRate(rate as f32) });
        Ok(())
    }

    fn set_volume(&mut self, volume: f64) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0) as f32
        } else {
            1.0
        };
        self.with_media(move |media| unsafe { media.player.setVolume(volume) });
    }

    fn set_muted(&mut self, muted: bool) {
        self.with_media(move |media| unsafe { media.player.setMuted(muted) });
    }
}

struct MacFrameExtractor {
    media: Arc<MainThreadBound<std::sync::Mutex<MacMedia>>>,
    timeout: Duration,
}

impl MacFrameExtractor {
    fn new(request: FrameExtractorBackendRequest) -> MediaResult<Self> {
        if request.source.network_options() != &Default::default() {
            return Err(MediaError::unsupported(
                "AVFoundation SystemBackend does not support custom HTTP headers or proxy options",
            ));
        }
        let uri = request.source.uri().to_owned();
        let media = run_on_main(move |mtm| {
            MacMedia::new(&uri, mtm).map(|media| {
                unsafe {
                    media.player.setMuted(true);
                    media.player.pause();
                }
                Arc::new(MainThreadBound::new(std::sync::Mutex::new(media), mtm))
            })
        })?;
        Ok(Self {
            media,
            timeout: request.timeout,
        })
    }

    fn frame(&mut self, position: Duration, mode: SeekMode) -> MediaResult<Arc<VideoFrame>> {
        self.media.get_on_main(|media| {
            let media = media.lock().expect("AVFoundation media mutex poisoned");
            media.seek(position, mode);
            unsafe { media.player.play() };
        });

        let deadline = Instant::now() + self.timeout;
        loop {
            let result = self.media.get_on_main(|media| {
                let mut media = media.lock().expect("AVFoundation media mutex poisoned");
                if let Some(error) = media.status_error() {
                    return Err(error);
                }
                media.copy_current_frame()
            })?;
            if let Some(frame) = result {
                self.media.get_on_main(|media| unsafe {
                    media
                        .lock()
                        .expect("AVFoundation media mutex poisoned")
                        .player
                        .pause()
                });
                return Ok(frame);
            }
            if Instant::now() >= deadline {
                self.media.get_on_main(|media| unsafe {
                    media
                        .lock()
                        .expect("AVFoundation media mutex poisoned")
                        .player
                        .pause()
                });
                return Err(MediaError::backend(
                    "timed out waiting for AVFoundation to decode a video frame",
                ));
            }
            std::thread::sleep(FRAME_POLL_INTERVAL);
        }
    }
}

impl FrameExtractionSession for MacFrameExtractor {
    fn initial_frame(&mut self) -> MediaResult<Arc<VideoFrame>> {
        self.frame(Duration::ZERO, SeekMode::KeyFrame)
    }

    fn frame_at(
        &mut self,
        position: Duration,
        seek_mode: SeekMode,
    ) -> MediaResult<Arc<VideoFrame>> {
        self.frame(position, seek_mode)
    }
}

fn start_frame_poll(
    media: Arc<MainThreadBound<std::sync::Mutex<MacMedia>>>,
    output: MediaOutputSink,
    pending_ready: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) -> MediaResult<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("gpui-video-avfoundation".into())
        .spawn(move || {
            let mut buffering = false;
            let mut ended = false;
            while !shutdown.load(Ordering::Acquire) && !output.is_closed() {
                let status = media.get_on_main(|media| {
                    let mut media = media.lock().expect("AVFoundation media mutex poisoned");
                    let error = media.status_error();
                    let ready = media.is_ready();
                    let buffering = media.is_buffering();
                    let ended = media.is_ended();
                    let frame = media.copy_current_frame();
                    (error, ready, buffering, ended, frame)
                });

                if let Some(error) = status.0 {
                    output.emit(MediaBackendEvent::Error(Arc::new(error)));
                    break;
                }
                match status.4 {
                    Ok(Some(frame)) => {
                        output.publish_video_frame(frame);
                        if pending_ready.swap(false, Ordering::AcqRel) {
                            output.emit(MediaBackendEvent::Ready);
                        }
                    }
                    Ok(None) => {
                        if status.1 && pending_ready.swap(false, Ordering::AcqRel) {
                            output.emit(MediaBackendEvent::Ready);
                        }
                    }
                    Err(error) => {
                        output.emit(MediaBackendEvent::Error(Arc::new(error)));
                        break;
                    }
                }
                if buffering != status.2 {
                    buffering = status.2;
                    output.emit(MediaBackendEvent::Buffering(if buffering {
                        0
                    } else {
                        100
                    }));
                }
                if status.3 && !ended {
                    ended = true;
                    output.emit(MediaBackendEvent::Ended);
                } else if !status.3 {
                    ended = false;
                }
                std::thread::sleep(FRAME_POLL_INTERVAL);
            }
        })
        .map_err(|error| MediaError::io("failed to start AVFoundation frame poll", error))
}

fn pixel_buffer_attributes() -> objc2::rc::Retained<NSDictionary<NSString, AnyObject>> {
    let pixel_format_key = NSString::from_str("PixelFormatType");
    let metal_compatibility_key = NSString::from_str("MetalCompatibility");
    let pixel_format =
        NSNumber::numberWithUnsignedInt(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange);
    let metal_compatible = NSNumber::numberWithBool(true);
    let pixel_format: &AnyObject = &pixel_format;
    let metal_compatible: &AnyObject = &metal_compatible;
    NSDictionary::<NSString, AnyObject>::from_slices::<NSString>(
        &[&pixel_format_key, &metal_compatibility_key],
        &[pixel_format, metal_compatible],
    )
}

fn video_frame_from_pixel_buffer(
    pixel_buffer: &CVPixelBuffer,
    handle: SurfaceHandle,
    sequence: u64,
    timestamp: CMTime,
) -> MediaResult<VideoFrame> {
    let width = CVPixelBufferGetWidth(pixel_buffer);
    let height = CVPixelBufferGetHeight(pixel_buffer);
    let width_pixels = i32::try_from(width)
        .map_err(|_| MediaError::backend("AVFoundation frame width is too large"))?;
    let height_pixels = i32::try_from(height)
        .map_err(|_| MediaError::backend("AVFoundation frame height is too large"))?;

    let pixel_format = CVPixelBufferGetPixelFormatType(pixel_buffer);
    let (format, color) = if pixel_format == kCVPixelFormatType_32BGRA {
        (SurfaceFormat::Bgra8, SurfaceColorInfo::default())
    } else if pixel_format == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange {
        (
            SurfaceFormat::Nv12,
            SurfaceColorInfo {
                matrix: YuvMatrix::Bt709,
                range: ColorRange::Limited,
            },
        )
    } else if pixel_format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange {
        (
            SurfaceFormat::Nv12,
            SurfaceColorInfo {
                matrix: YuvMatrix::Bt709,
                range: ColorRange::Full,
            },
        )
    } else {
        return Err(MediaError::unsupported(format!(
            "AVFoundation produced unsupported CoreVideo format {pixel_format:#010x}",
        )));
    };
    if format == SurfaceFormat::Nv12 && CVPixelBufferGetPlaneCount(pixel_buffer) < 2 {
        return Err(MediaError::backend(
            "AVFoundation produced an invalid single-plane NV12 buffer",
        ));
    }

    let native_pixel_buffer = unsafe {
        NativePixelBuffer::wrap_under_get_rule(
            pixel_buffer as *const CVPixelBuffer as CVPixelBufferRef,
        )
    };
    let frame_size = size(DevicePixels(width_pixels), DevicePixels(height_pixels));
    let surface = SurfaceFrame::from_core_video(
        handle,
        sequence,
        Bounds::new(point(DevicePixels(0), DevicePixels(0)), frame_size),
        frame_size,
        format,
        unsafe { CoreVideoHandle::new(native_pixel_buffer) },
        color,
    )
    .map_err(|error| {
        MediaError::from_error(
            MediaErrorKind::VideoOutput,
            MediaRecovery::None,
            "GPUI rejected an AVFoundation CoreVideo frame",
            error,
        )
    })?;

    Ok(VideoFrame::new(
        Arc::new(surface),
        duration_from_cm_time(timestamp),
        None,
    ))
}

fn cm_time(duration: Duration) -> CMTime {
    unsafe { CMTime::with_seconds(duration.as_secs_f64(), MACOS_TIME_SCALE) }
}

fn duration_from_cm_time(time: CMTime) -> Option<Duration> {
    let seconds = unsafe { time.seconds() };
    (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds))
}

fn reject_unsupported_network_options(request: &MediaPlaybackRequest) -> MediaResult<()> {
    if request.source.network_options() != &Default::default() {
        return Err(MediaError::unsupported(
            "AVFoundation SystemBackend does not support custom HTTP headers or proxy options",
        ));
    }
    Ok(())
}
