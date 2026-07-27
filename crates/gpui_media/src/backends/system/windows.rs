use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use gpui::{DevicePixels, SurfaceFrame, SurfaceHandle, size};
use windows::{
    Win32::{
        Foundation::{RECT, S_FALSE},
        Graphics::{
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Imaging::{
                CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICBitmap,
                IWICImagingFactory, WICBitmapCacheOnLoad, WICBitmapLockRead, WICRect,
            },
        },
        Media::MediaFoundation::{
            CLSID_MFMediaEngineClassFactory, IMFMediaEngine, IMFMediaEngineClassFactory,
            IMFMediaEngineEx, IMFMediaEngineNotify, IMFMediaEngineNotify_Impl,
            MF_MEDIA_ENGINE_CALLBACK, MF_MEDIA_ENGINE_EVENT_BUFFERINGENDED,
            MF_MEDIA_ENGINE_EVENT_BUFFERINGSTARTED, MF_MEDIA_ENGINE_EVENT_CANPLAY,
            MF_MEDIA_ENGINE_EVENT_CANPLAYTHROUGH, MF_MEDIA_ENGINE_EVENT_ENDED,
            MF_MEDIA_ENGINE_EVENT_ERROR, MF_MEDIA_ENGINE_EVENT_FIRSTFRAMEREADY,
            MF_MEDIA_ENGINE_EVENT_LOADEDDATA, MF_MEDIA_ENGINE_EVENT_PLAYING,
            MF_MEDIA_ENGINE_EVENT_SEEKED, MF_MEDIA_ENGINE_EVENT_STALLED,
            MF_MEDIA_ENGINE_EVENT_WAITING, MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT, MF_VERSION, MFARGB,
            MFCreateAttributes, MFSTARTUP_FULL, MFShutdown, MFStartup, MFVideoNormalizedRect,
        },
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
    },
    core::{BSTR, Interface, implement},
};

use crate::{
    FrameExtractionSession, FrameExtractorBackendRequest, MediaBackendEvent, MediaCapabilities,
    MediaError, MediaErrorKind, MediaOutputSink, MediaPlaybackRequest, MediaPlaybackSession,
    MediaRecovery, MediaResult, PlaybackTimeline, SeekMode, VideoFrame,
};

const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(5);

pub(super) fn initialize() -> MediaResult<()> {
    Ok(())
}

pub(super) fn open_playback(
    request: MediaPlaybackRequest,
    output: MediaOutputSink,
) -> MediaResult<Box<dyn MediaPlaybackSession>> {
    WindowsPlayback::new(request, output)
        .map(|playback| Box::new(playback) as Box<dyn MediaPlaybackSession>)
}

pub(super) fn open_frame_extractor(
    request: FrameExtractorBackendRequest,
) -> MediaResult<Box<dyn FrameExtractionSession>> {
    WindowsFrameExtractor::new(request)
        .map(|extractor| Box::new(extractor) as Box<dyn FrameExtractionSession>)
}

enum Command {
    Play(Sender<MediaResult<()>>),
    Pause(Sender<MediaResult<()>>),
    Reload {
        autoplay: bool,
        response: Sender<MediaResult<()>>,
    },
    Timeline(Sender<PlaybackTimeline>),
    Seek {
        position: Duration,
        response: Sender<MediaResult<()>>,
    },
    Step {
        frames: u64,
        response: Sender<MediaResult<()>>,
    },
    Rate {
        rate: f64,
        response: Sender<MediaResult<()>>,
    },
    Volume(f64),
    Muted(bool),
    Shutdown,
}

struct WindowsPlayback {
    commands: Sender<Command>,
    worker: Option<JoinHandle<()>>,
}

impl WindowsPlayback {
    fn new(request: MediaPlaybackRequest, output: MediaOutputSink) -> MediaResult<Self> {
        reject_unsupported_network_options(&request)?;
        let (commands, command_rx) = mpsc::channel();
        let (initialized_tx, initialized_rx) = mpsc::sync_channel(1);
        let source_uri = request.source.uri().to_owned();
        let worker = std::thread::Builder::new()
            .name("gpui-video-media-foundation".into())
            .spawn(move || {
                let result = MediaFoundationWorker::new(&source_uri, output);
                match result {
                    Ok(mut worker) => {
                        let _ = initialized_tx.send(Ok(()));
                        worker.run(command_rx);
                    }
                    Err(error) => {
                        let _ = initialized_tx.send(Err(error));
                    }
                }
            })
            .map_err(|error| MediaError::io("failed to start Media Foundation worker", error))?;

        initialized_rx.recv().map_err(|error| {
            MediaError::io(
                "Media Foundation worker stopped during initialization",
                error,
            )
        })??;

        Ok(Self {
            commands,
            worker: Some(worker),
        })
    }

    fn request(&self, command: impl FnOnce(Sender<MediaResult<()>>) -> Command) -> MediaResult<()> {
        let (response_tx, response_rx) = mpsc::channel();
        self.commands
            .send(command(response_tx))
            .map_err(|error| MediaError::io("Media Foundation worker stopped", error))?;
        response_rx
            .recv()
            .map_err(|error| MediaError::io("Media Foundation worker stopped", error))?
    }
}

impl Drop for WindowsPlayback {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl MediaPlaybackSession for WindowsPlayback {
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
        self.request(Command::Play)
    }

    fn pause(&mut self) -> MediaResult<()> {
        self.request(Command::Pause)
    }

    fn reload(&mut self, autoplay: bool) -> MediaResult<()> {
        self.request(|response| Command::Reload { autoplay, response })
    }

    fn timeline(&self) -> PlaybackTimeline {
        let (response_tx, response_rx) = mpsc::channel();
        if self.commands.send(Command::Timeline(response_tx)).is_err() {
            return PlaybackTimeline::default();
        }
        response_rx.recv().unwrap_or_default()
    }

    fn seek_to(&mut self, position: Duration, _mode: SeekMode) -> MediaResult<()> {
        self.request(|response| Command::Seek { position, response })
    }

    fn step_forward(&mut self, frames: u64) -> MediaResult<()> {
        self.request(|response| Command::Step { frames, response })
    }

    fn set_playback_rate(&mut self, rate: f64) -> MediaResult<()> {
        if !rate.is_finite() || rate <= 0.0 {
            return Err(MediaError::invalid_input(
                "playback rate must be finite and greater than zero",
            ));
        }
        self.request(|response| Command::Rate { rate, response })
    }

    fn set_volume(&mut self, volume: f64) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let _ = self.commands.send(Command::Volume(volume));
    }

    fn set_muted(&mut self, muted: bool) {
        let _ = self.commands.send(Command::Muted(muted));
    }
}

#[derive(Clone, Copy)]
struct NativeEvent {
    event: u32,
    param1: usize,
    _param2: u32,
}

#[implement(IMFMediaEngineNotify)]
struct MediaEngineNotify {
    events: Sender<NativeEvent>,
}

#[allow(non_snake_case)]
impl IMFMediaEngineNotify_Impl for MediaEngineNotify_Impl {
    fn EventNotify(&self, event: u32, param1: usize, param2: u32) -> windows::core::Result<()> {
        let _ = self.events.send(NativeEvent {
            event,
            param1,
            _param2: param2,
        });
        Ok(())
    }
}

struct MediaFoundationWorker {
    engine: IMFMediaEngine,
    imaging_factory: IWICImagingFactory,
    native_events: Receiver<NativeEvent>,
    output: MediaOutputSink,
    bitmap: Option<(u32, u32, IWICBitmap)>,
    surface_handle: SurfaceHandle,
    sequence: u64,
    com_initialized: bool,
    mf_initialized: bool,
}

impl MediaFoundationWorker {
    fn new(source_uri: &str, output: MediaOutputSink) -> MediaResult<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|error| windows_backend_error("failed to initialize COM", error))?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(windows_backend_error(
                    "failed to initialize Media Foundation",
                    error,
                ));
            }
        }

        let result = (|| unsafe {
            let factory: IMFMediaEngineClassFactory =
                CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)
                    .map_err(|error| {
                        windows_backend_error(
                            "failed to create Media Foundation engine factory",
                            error,
                        )
                    })?;
            let imaging_factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(
                    |error| {
                        windows_backend_error("failed to create Windows Imaging factory", error)
                    },
                )?;

            let (event_tx, native_events) = mpsc::channel();
            let notify: IMFMediaEngineNotify = MediaEngineNotify { events: event_tx }.into();
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 2).map_err(|error| {
                windows_backend_error("failed to create Media Foundation attributes", error)
            })?;
            let attributes = attributes
                .ok_or_else(|| MediaError::backend("Media Foundation returned no attributes"))?;
            attributes
                .SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &notify)
                .map_err(|error| {
                    windows_backend_error("failed to set Media Foundation callback", error)
                })?;
            attributes
                .SetUINT32(
                    &MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT,
                    DXGI_FORMAT_B8G8R8A8_UNORM.0 as u32,
                )
                .map_err(|error| {
                    windows_backend_error("failed to select Media Foundation video output", error)
                })?;

            let engine = factory.CreateInstance(0, &attributes).map_err(|error| {
                windows_backend_error("failed to create Media Foundation media engine", error)
            })?;
            engine.SetSource(&BSTR::from(source_uri)).map_err(|error| {
                windows_backend_error("Media Foundation rejected the media source", error)
            })?;
            engine.Load().map_err(|error| {
                windows_backend_error("failed to load the Media Foundation source", error)
            })?;

            Ok(Self {
                engine,
                imaging_factory,
                native_events,
                output,
                bitmap: None,
                surface_handle: SurfaceHandle::new(),
                sequence: 1,
                com_initialized: true,
                mf_initialized: true,
            })
        })();

        if result.is_err() {
            unsafe {
                let _ = MFShutdown();
                CoUninitialize();
            }
        }
        result
    }

    fn run(&mut self, commands: Receiver<Command>) {
        loop {
            match commands.recv_timeout(FRAME_POLL_INTERVAL) {
                Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Ok(command) => self.handle_command(command),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }

            while let Ok(event) = self.native_events.try_recv() {
                self.handle_native_event(event);
            }
            if self.output.is_closed() {
                break;
            }
            match self.copy_current_frame() {
                Ok(Some(frame)) => {
                    self.output.publish_video_frame(Arc::new(frame));
                }
                Ok(None) => {}
                Err(error) => {
                    self.output.emit(MediaBackendEvent::Error(Arc::new(error)));
                    break;
                }
            }
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Play(response) => {
                let _ = response.send(unsafe { self.engine.Play() }.map_err(|error| {
                    windows_backend_error("failed to start Media Foundation playback", error)
                }));
            }
            Command::Pause(response) => {
                let _ = response.send(unsafe { self.engine.Pause() }.map_err(|error| {
                    windows_backend_error("failed to pause Media Foundation playback", error)
                }));
            }
            Command::Reload { autoplay, response } => {
                let result = (|| unsafe {
                    self.engine.SetCurrentTime(0.0).map_err(|error| {
                        windows_backend_error("failed to rewind Media Foundation playback", error)
                    })?;
                    if autoplay {
                        self.engine.Play()
                    } else {
                        self.engine.Pause()
                    }
                    .map_err(|error| {
                        windows_backend_error("failed to reload Media Foundation playback", error)
                    })
                })();
                let _ = response.send(result);
            }
            Command::Timeline(response) => {
                let _ = response.send(self.timeline());
            }
            Command::Seek { position, response } => {
                let result = unsafe { self.engine.SetCurrentTime(position.as_secs_f64()) }.map_err(
                    |error| {
                        windows_backend_error("failed to seek Media Foundation playback", error)
                    },
                );
                let _ = response.send(result);
            }
            Command::Step { frames, response } => {
                let result = if frames == 0 {
                    Err(MediaError::invalid_input(
                        "frame step amount must be greater than zero",
                    ))
                } else {
                    (|| unsafe {
                        self.engine.Pause().map_err(|error| {
                            windows_backend_error(
                                "failed to pause before Media Foundation frame step",
                                error,
                            )
                        })?;
                        let engine: IMFMediaEngineEx = self.engine.cast().map_err(|error| {
                            windows_backend_error(
                                "Media Foundation frame stepping is unavailable",
                                error,
                            )
                        })?;
                        for _ in 0..frames {
                            engine.FrameStep(true).map_err(|error| {
                                windows_backend_error(
                                    "Media Foundation rejected frame stepping",
                                    error,
                                )
                            })?;
                        }
                        Ok(())
                    })()
                };
                let _ = response.send(result);
            }
            Command::Rate { rate, response } => {
                let result = unsafe { self.engine.SetPlaybackRate(rate) }.map_err(|error| {
                    windows_backend_error("failed to change Media Foundation playback rate", error)
                });
                let _ = response.send(result);
            }
            Command::Volume(volume) => {
                let _ = unsafe { self.engine.SetVolume(volume) };
            }
            Command::Muted(muted) => {
                let _ = unsafe { self.engine.SetMuted(muted) };
            }
            Command::Shutdown => {}
        }
    }

    fn timeline(&self) -> PlaybackTimeline {
        let position = unsafe { self.engine.GetCurrentTime() };
        let duration = unsafe { self.engine.GetDuration() };
        let position = seconds_duration(position).unwrap_or_default();
        let duration = seconds_duration(duration);
        let seekable = unsafe {
            self.engine
                .GetSeekable()
                .is_ok_and(|ranges| ranges.GetLength() > 0)
        };
        PlaybackTimeline::new(position, duration, seekable)
    }

    fn handle_native_event(&self, event: NativeEvent) {
        let event_code = event.event as i32;
        if [
            MF_MEDIA_ENGINE_EVENT_LOADEDDATA.0,
            MF_MEDIA_ENGINE_EVENT_CANPLAY.0,
            MF_MEDIA_ENGINE_EVENT_CANPLAYTHROUGH.0,
            MF_MEDIA_ENGINE_EVENT_FIRSTFRAMEREADY.0,
            MF_MEDIA_ENGINE_EVENT_SEEKED.0,
        ]
        .contains(&event_code)
        {
            self.output.emit(MediaBackendEvent::Ready);
        } else if [
            MF_MEDIA_ENGINE_EVENT_WAITING.0,
            MF_MEDIA_ENGINE_EVENT_STALLED.0,
            MF_MEDIA_ENGINE_EVENT_BUFFERINGSTARTED.0,
        ]
        .contains(&event_code)
        {
            self.output.emit(MediaBackendEvent::Buffering(0));
        } else if [
            MF_MEDIA_ENGINE_EVENT_PLAYING.0,
            MF_MEDIA_ENGINE_EVENT_BUFFERINGENDED.0,
        ]
        .contains(&event_code)
        {
            self.output.emit(MediaBackendEvent::Buffering(100));
        } else if event_code == MF_MEDIA_ENGINE_EVENT_ENDED.0 {
            self.output.emit(MediaBackendEvent::Ended);
        } else if event_code == MF_MEDIA_ENGINE_EVENT_ERROR.0 {
            self.output
                .emit(MediaBackendEvent::Error(Arc::new(MediaError::new(
                    MediaErrorKind::Decode,
                    format!("Media Foundation playback error {:#010x}", event.param1),
                    MediaRecovery::Reload,
                ))));
        }
    }

    fn copy_current_frame(&mut self) -> MediaResult<Option<VideoFrame>> {
        if !unsafe { self.engine.HasVideo() }.as_bool() {
            return Ok(None);
        }

        let mut presentation_ticks = 0i64;
        let status = unsafe {
            (Interface::vtable(&self.engine).OnVideoStreamTick)(
                Interface::as_raw(&self.engine),
                &mut presentation_ticks,
            )
        };
        if status == S_FALSE {
            return Ok(None);
        }
        status.ok().map_err(|error| {
            windows_backend_error("failed to poll Media Foundation video", error)
        })?;

        let mut width = 0;
        let mut height = 0;
        unsafe {
            self.engine
                .GetNativeVideoSize(Some(&mut width), Some(&mut height))
        }
        .map_err(|error| {
            windows_backend_error("failed to query Media Foundation video size", error)
        })?;
        if width == 0 || height == 0 {
            return Ok(None);
        }

        let recreate = self
            .bitmap
            .as_ref()
            .is_none_or(|(current_width, current_height, _)| {
                *current_width != width || *current_height != height
            });
        if recreate {
            let bitmap = unsafe {
                self.imaging_factory.CreateBitmap(
                    width,
                    height,
                    &GUID_WICPixelFormat32bppBGRA,
                    WICBitmapCacheOnLoad,
                )
            }
            .map_err(|error| {
                windows_backend_error("failed to allocate Media Foundation output bitmap", error)
            })?;
            self.bitmap = Some((width, height, bitmap));
        }
        let bitmap = &self
            .bitmap
            .as_ref()
            .expect("Media Foundation bitmap was initialized")
            .2;

        let source = MFVideoNormalizedRect {
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
        };
        let destination = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(width)
                .map_err(|_| MediaError::backend("Media Foundation width is too large"))?,
            bottom: i32::try_from(height)
                .map_err(|_| MediaError::backend("Media Foundation height is too large"))?,
        };
        let border = MFARGB::default();
        unsafe {
            self.engine
                .TransferVideoFrame(bitmap, Some(&source), &destination, Some(&border))
        }
        .map_err(|error| {
            windows_backend_error("failed to transfer Media Foundation video frame", error)
        })?;

        let lock_rect = WICRect {
            X: 0,
            Y: 0,
            Width: destination.right,
            Height: destination.bottom,
        };
        let lock =
            unsafe { bitmap.Lock(&lock_rect, WICBitmapLockRead.0 as u32) }.map_err(|error| {
                windows_backend_error("failed to lock Media Foundation output bitmap", error)
            })?;
        let stride = unsafe { lock.GetStride() }.map_err(|error| {
            windows_backend_error("failed to query Media Foundation output stride", error)
        })?;
        let mut buffer_size = 0;
        let mut buffer = std::ptr::null_mut();
        unsafe { lock.GetDataPointer(&mut buffer_size, &mut buffer) }.map_err(|error| {
            windows_backend_error("failed to map Media Foundation output bitmap", error)
        })?;
        let required = usize::try_from(stride)
            .ok()
            .and_then(|stride| stride.checked_mul(height as usize))
            .ok_or_else(|| MediaError::backend("Media Foundation frame size overflow"))?;
        if buffer.is_null() || (buffer_size as usize) < required {
            return Err(MediaError::backend(
                "Media Foundation returned an incomplete output bitmap",
            ));
        }
        let bytes = unsafe { std::slice::from_raw_parts(buffer, required) }.to_vec();
        drop(lock);

        let frame_size = size(
            DevicePixels(destination.right),
            DevicePixels(destination.bottom),
        );
        let surface = SurfaceFrame::bgra(
            self.surface_handle.clone(),
            self.sequence,
            frame_size,
            bytes,
            stride,
        )
        .map_err(|error| {
            MediaError::from_error(
                MediaErrorKind::VideoOutput,
                MediaRecovery::None,
                "GPUI rejected a Media Foundation video frame",
                error,
            )
        })?;
        self.sequence = self.sequence.wrapping_add(1);
        let timestamp = u64::try_from(presentation_ticks)
            .ok()
            .map(|ticks| Duration::from_nanos(ticks.saturating_mul(100)));
        Ok(Some(VideoFrame::new(Arc::new(surface), timestamp, None)))
    }
}

impl Drop for MediaFoundationWorker {
    fn drop(&mut self) {
        unsafe {
            let _ = self.engine.Shutdown();
            if self.mf_initialized {
                let _ = MFShutdown();
            }
            if self.com_initialized {
                CoUninitialize();
            }
        }
    }
}

struct WindowsFrameExtractor {
    playback: WindowsPlayback,
    output: crate::media_backend::MediaOutput,
    timeout: Duration,
}

impl WindowsFrameExtractor {
    fn new(request: FrameExtractorBackendRequest) -> MediaResult<Self> {
        let (sink, output) = MediaOutputSink::channel();
        let playback = WindowsPlayback::new(
            MediaPlaybackRequest {
                source: request.source,
                gpu_specs: None,
            },
            sink,
        )?;
        let mut extractor = Self {
            playback,
            output,
            timeout: request.timeout,
        };
        extractor.playback.set_muted(true);
        extractor.playback.pause()?;
        Ok(extractor)
    }

    fn frame(&mut self, position: Duration, mode: SeekMode) -> MediaResult<Arc<VideoFrame>> {
        while self.output.video_frames.try_recv().is_ok() {}
        while self.output.events.try_recv().is_ok() {}
        self.playback.seek_to(position, mode)?;
        self.playback.play()?;

        let deadline = Instant::now() + self.timeout;
        let mut seek_ready = false;
        loop {
            while let Ok(event) = self.output.events.try_recv() {
                match event {
                    MediaBackendEvent::Ready => seek_ready = true,
                    MediaBackendEvent::Error(error) => return Err((*error).clone()),
                    _ => {}
                }
            }
            if let Ok(frame) = self.output.video_frames.try_recv()
                && (seek_ready || frame.timestamp().is_some())
            {
                self.playback.pause()?;
                return Ok(frame);
            }
            if Instant::now() >= deadline {
                self.playback.pause()?;
                return Err(MediaError::backend(
                    "timed out waiting for Media Foundation to decode a video frame",
                ));
            }
            std::thread::sleep(FRAME_POLL_INTERVAL);
        }
    }
}

impl FrameExtractionSession for WindowsFrameExtractor {
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

fn seconds_duration(seconds: f64) -> Option<Duration> {
    (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds))
}

fn windows_backend_error(context: impl Into<String>, error: windows::core::Error) -> MediaError {
    MediaError::from_error(MediaErrorKind::Backend, MediaRecovery::None, context, error)
}

fn reject_unsupported_network_options(request: &MediaPlaybackRequest) -> MediaResult<()> {
    if request.source.network_options() != &Default::default() {
        return Err(MediaError::unsupported(
            "Media Foundation SystemBackend does not support custom HTTP headers or proxy options",
        ));
    }
    Ok(())
}
