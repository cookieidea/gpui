//! Pure-Rust decv playback and frame-extraction backend.

use std::{
    collections::VecDeque,
    fs::File,
    num::NonZeroUsize,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

mod audio_output;

use audio_output::{AudioClock, AudioOutput};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use decv::{
    AudioDecodeInputStatus, AudioDecodeOutput, AudioDecoder, AudioFormat, AudioPacketCursor,
    ColorInfo, ColorMatrix as DecvColorMatrix, ColorRange as DecvColorRange, CpuBuffer,
    DecodeInputStatus, DecodeOutput, DecodedVideoFrame, EncodedAudioPacket, EncodedVideoPacket,
    FrameStorage, H264Decoder, H264Mp4SeekController, H264Parallelism, HttpRangeInput, MediaInput,
    MediaTime, Mp4Demuxer, PacketCursor, PixelFormat, RangeCacheConfig, Size as DecvSize,
    SoftwareAudioDecoder, Track, TrackKind, VideoCodec, VideoDecoder, VideoDecoderConfig,
    Vp9Decoder,
};
use gpui::{
    Bounds, ColorRange, DevicePixels, SurfaceColorInfo, SurfaceFormat, SurfaceFrame, SurfaceHandle,
    SurfacePlane, YuvMatrix, point, size,
};

use crate::{
    FrameExtractionSession, FrameExtractorBackendRequest, MediaBackend, MediaBackendEvent,
    MediaCapabilities, MediaError, MediaErrorKind, MediaOutputSink, MediaPlaybackRequest,
    MediaPlaybackSession, MediaRecovery, MediaResult, MediaSource, PlaybackTimeline, SeekMode,
    VideoFrame,
};

const MEDIA_TIME_SCALE: u32 = 1_000_000_000;
const AUDIO_BUFFER_TARGET: Duration = Duration::from_secs(2);
const AUDIO_PREROLL_TARGET: Duration = Duration::from_millis(100);
const AUDIO_CLOCK_POLL_INTERVAL: Duration = Duration::from_millis(5);
const VIDEO_SEEK_CHECKPOINT_LIMIT: usize = 4;
const VIDEO_SEEK_CHECKPOINT_REFERENCE_BYTES: usize = 128 * 1024 * 1024;
const DEFAULT_INTERACTIVE_SEEK_MAXIMUM_INPUT_SAMPLES: usize = 12;
// Keep decv's cross-picture work fed while the front frame waits for presentation.
const VIDEO_DECODE_QUEUE_CAPACITY: usize = 4;
// Avoid synchronous request churn and A/V cursor cache thrashing during HTTP playback.
const HTTP_CACHE_BLOCK_BYTES: usize = 1024 * 1024;
const HTTP_CACHE_MAXIMUM_BLOCKS: usize = 64;
// Keep enough compressed media ahead to hide ordinary HTTP Range latency without
// reserving a large span that makes a reader wait for an oversized response.
const HTTP_PREFETCH_WINDOW_BYTES: usize = 8 * 1024 * 1024;
const HTTP_PREFETCH_REARM_BYTES: u64 = 4 * 1024 * 1024;

type DecvMediaInput = Arc<dyn MediaInput>;
type DecvDemuxer = Mp4Demuxer<DecvMediaInput>;

#[derive(Debug)]
enum DecvVideoDecoder {
    H264(H264Decoder),
    Vp9(Vp9Decoder),
}

impl DecvVideoDecoder {
    fn new(config: VideoDecoderConfig, parallelism: DecvParallelism) -> MediaResult<Self> {
        match config.codec {
            VideoCodec::H264 => {
                let mut decoder = H264Decoder::new();
                decoder
                    .set_parallelism(parallelism.into())
                    .map_err(|error| {
                        decv_decode_error("failed to configure decv H.264 parallelism", error)
                    })?;
                decoder.configure(config).map_err(|error| {
                    MediaError::new(
                        MediaErrorKind::UnsupportedCodec,
                        format!("failed to configure decv H.264 decoder: {error:?}"),
                        MediaRecovery::None,
                    )
                })?;
                Ok(Self::H264(decoder))
            }
            VideoCodec::Vp9 => {
                let mut decoder = Vp9Decoder::new();
                decoder.configure(config).map_err(|error| {
                    MediaError::new(
                        MediaErrorKind::UnsupportedCodec,
                        format!("failed to configure decv VP9 decoder: {error:?}"),
                        MediaRecovery::None,
                    )
                })?;
                Ok(Self::Vp9(decoder))
            }
            codec => Err(MediaError::new(
                MediaErrorKind::UnsupportedCodec,
                format!("decv does not support MP4 video codec {codec:?}"),
                MediaRecovery::None,
            )),
        }
    }

    fn codec(&self) -> VideoCodec {
        match self {
            Self::H264(_) => VideoCodec::H264,
            Self::Vp9(_) => VideoCodec::Vp9,
        }
    }

    fn h264_mut(&mut self) -> Option<&mut H264Decoder> {
        match self {
            Self::H264(decoder) => Some(decoder),
            Self::Vp9(_) => None,
        }
    }

    fn receive_frame(&mut self) -> MediaResult<DecodeOutput> {
        match self {
            Self::H264(decoder) => decoder.receive_frame().map_err(|error| {
                decv_decode_error("decv H.264 failed while receiving a frame", error)
            }),
            Self::Vp9(decoder) => decoder.receive_frame().map_err(|error| {
                decv_decode_error("decv VP9 failed while receiving a frame", error)
            }),
        }
    }

    fn send_packet(&mut self, packet: EncodedVideoPacket) -> MediaResult<DecodeInputStatus> {
        match self {
            Self::H264(decoder) => decoder
                .send_packet(packet)
                .map_err(|error| decv_decode_error("decv H.264 failed to decode a packet", error)),
            Self::Vp9(decoder) => decoder
                .send_packet(packet)
                .map_err(|error| decv_decode_error("decv VP9 failed to decode a packet", error)),
        }
    }

    fn flush(&mut self) {
        match self {
            Self::H264(decoder) => decoder.flush(),
            Self::Vp9(decoder) => decoder.flush(),
        }
    }

    fn drain(&mut self) -> MediaResult<()> {
        match self {
            Self::H264(decoder) => decoder
                .drain()
                .map_err(|error| decv_decode_error("decv H.264 failed to drain", error)),
            Self::Vp9(decoder) => decoder
                .drain()
                .map_err(|error| decv_decode_error("decv VP9 failed to drain", error)),
        }
    }
}

/// Reconstruction parallelism used by the built-in `decv` H.264 decoder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecvParallelism {
    /// Decode and reconstruct on the playback worker thread.
    Serial,
    /// Let `decv` select a conservative worker count for the coded size.
    #[default]
    Auto,
    /// Use exactly this many reconstruction threads.
    Threads(NonZeroUsize),
}

impl From<DecvParallelism> for H264Parallelism {
    fn from(value: DecvParallelism) -> Self {
        match value {
            DecvParallelism::Serial => Self::Serial,
            DecvParallelism::Auto => Self::Auto,
            DecvParallelism::Threads(threads) => Self::Threads(threads),
        }
    }
}

/// Options applied when a [`DecvBackend`] opens a playback session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecvBackendOptions {
    parallelism: DecvParallelism,
    interactive_seek_maximum_input_samples: usize,
}

impl Default for DecvBackendOptions {
    fn default() -> Self {
        Self {
            parallelism: DecvParallelism::default(),
            interactive_seek_maximum_input_samples: DEFAULT_INTERACTIVE_SEEK_MAXIMUM_INPUT_SAMPLES,
        }
    }
}

impl DecvBackendOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parallelism(mut self, parallelism: DecvParallelism) -> Self {
        self.parallelism = parallelism;
        self
    }

    pub fn selected_parallelism(self) -> DecvParallelism {
        self.parallelism
    }

    /// Sets the compressed-sample budget used by interactive seeks.
    pub fn interactive_seek_maximum_input_samples(mut self, maximum: usize) -> Self {
        self.interactive_seek_maximum_input_samples = maximum;
        self
    }

    pub fn selected_interactive_seek_maximum_input_samples(self) -> usize {
        self.interactive_seek_maximum_input_samples
    }
}

/// Pure-Rust MP4 H.264/VP9 and AAC playback backed by `decv`.
///
/// This backend is intentionally opt-in while `decv` is pre-1.0. Its types
/// remain confined to this module so applications depend only on
/// `gpui_media`'s stable backend boundary.
#[derive(Clone, Copy, Debug, Default)]
pub struct DecvBackend {
    options: DecvBackendOptions,
}

impl DecvBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: DecvBackendOptions) -> Self {
        Self { options }
    }

    pub fn parallelism(mut self, parallelism: DecvParallelism) -> Self {
        self.options = self.options.parallelism(parallelism);
        self
    }

    pub fn interactive_seek_maximum_input_samples(mut self, maximum: usize) -> Self {
        self.options = self.options.interactive_seek_maximum_input_samples(maximum);
        self
    }

    pub fn options(&self) -> DecvBackendOptions {
        self.options
    }
}

impl MediaBackend for DecvBackend {
    fn name(&self) -> &'static str {
        "decv"
    }

    fn open_playback(
        &self,
        request: MediaPlaybackRequest,
        output: MediaOutputSink,
    ) -> MediaResult<Box<dyn MediaPlaybackSession>> {
        DecvSession::open(request.source, output, self.options)
            .map(|session| Box::new(session) as Box<dyn MediaPlaybackSession>)
    }

    fn open_frame_extractor(
        &self,
        request: FrameExtractorBackendRequest,
    ) -> MediaResult<Box<dyn FrameExtractionSession>> {
        Ok(Box::new(DecvFrameExtractionSession::new(
            request.source,
            request.timeout,
            self.options,
        )))
    }
}

struct DecvSession {
    commands: mpsc::Sender<Command>,
    timeline: Arc<Mutex<TimelineState>>,
    latest_seek_generation: Arc<AtomicU64>,
    has_audio: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl DecvSession {
    fn open(
        source: MediaSource,
        output: MediaOutputSink,
        options: DecvBackendOptions,
    ) -> MediaResult<Self> {
        let timeline = Arc::new(Mutex::new(TimelineState::new(None)));
        let (commands, receiver) = mpsc::channel();
        let worker_timeline = timeline.clone();
        let latest_seek_generation = Arc::new(AtomicU64::new(0));
        let worker_latest_seek_generation = latest_seek_generation.clone();
        let has_audio = Arc::new(AtomicBool::new(false));
        let worker_has_audio = has_audio.clone();
        let worker_source = source;
        let worker = thread::Builder::new()
            .name("gpui-video-decv".to_owned())
            .spawn(move || {
                let result = (|| {
                    let prepared = probe_mp4(&worker_source)?;
                    worker_timeline
                        .lock_unpoisoned()
                        .set_duration(prepared.probe.duration);
                    worker_has_audio.store(
                        prepared.probe.audio_track_index.is_some(),
                        Ordering::Release,
                    );
                    playback_worker(
                        prepared.demuxer,
                        prepared.probe.video_track_index,
                        prepared.probe.audio_track_index,
                        prepared.network,
                        receiver,
                        &worker_timeline,
                        worker_latest_seek_generation,
                        &output,
                        options.parallelism,
                        options.interactive_seek_maximum_input_samples,
                    )
                })();
                if let Err(error) = result {
                    let error = MediaError::new(
                        error.kind,
                        worker_source.redact_error_message(&error.message),
                        error.recovery,
                    );
                    log::error!(
                        "decv playback failed for {}: {error}",
                        worker_source.display_name()
                    );
                    output.emit(MediaBackendEvent::Error(Arc::new(error)));
                }
            })
            .map_err(|error| MediaError::io("failed to start decv playback thread", error))?;

        Ok(Self {
            commands,
            timeline,
            latest_seek_generation,
            has_audio,
            worker: Some(worker),
        })
    }

    fn send(&self, command: Command) -> MediaResult<()> {
        self.commands.send(command).map_err(|_| {
            MediaError::new(
                MediaErrorKind::Backend,
                "decv playback thread has stopped",
                MediaRecovery::Retry,
            )
        })
    }
}

impl MediaPlaybackSession for DecvSession {
    fn capabilities(&self) -> MediaCapabilities {
        MediaCapabilities {
            video: true,
            audio: self.has_audio.load(Ordering::Acquire),
            seeking: true,
            accurate_seeking: true,
            frame_extraction: true,
            ..MediaCapabilities::default()
        }
    }

    fn play(&mut self) -> MediaResult<()> {
        self.timeline
            .lock_unpoisoned()
            .set_playing(true, Instant::now());
        self.send(Command::Play)
    }

    fn pause(&mut self) -> MediaResult<()> {
        self.timeline
            .lock_unpoisoned()
            .set_playing(false, Instant::now());
        self.send(Command::Pause)
    }

    fn timeline(&self) -> PlaybackTimeline {
        self.timeline.lock_unpoisoned().snapshot(Instant::now())
    }

    fn reload(&mut self, autoplay: bool) -> MediaResult<()> {
        let generation = self
            .latest_seek_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        self.timeline
            .lock_unpoisoned()
            .begin_seek(Duration::ZERO, autoplay, Instant::now());
        self.send(Command::Reload {
            autoplay,
            generation,
        })
    }

    fn seek_to(&mut self, position: Duration, mode: SeekMode) -> MediaResult<()> {
        let generation = self
            .latest_seek_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let mut timeline = self.timeline.lock_unpoisoned();
        let target = timeline
            .duration
            .map_or(position, |duration| position.min(duration));
        let playing = timeline.playing;
        timeline.begin_seek(target, playing, Instant::now());
        drop(timeline);
        self.send(Command::Seek {
            position: target,
            mode,
            generation,
        })
    }

    fn set_volume(&mut self, volume: f64) {
        let _ = self.commands.send(Command::SetVolume(volume));
    }

    fn set_muted(&mut self, muted: bool) {
        let _ = self.commands.send(Command::SetMuted(muted));
    }
}

impl Drop for DecvSession {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        // A pure CPU decoder can be inside a long-running frame reconstruction.
        // Detach instead of blocking GPUI's entity teardown; the command makes
        // the worker exit at its next control boundary.
        self.worker.take();
    }
}

struct DecvFrameExtractionSession {
    source: MediaSource,
    timeout: Duration,
    options: DecvBackendOptions,
    state: Option<DecvFrameExtractorState>,
}

impl DecvFrameExtractionSession {
    fn new(source: MediaSource, timeout: Duration, options: DecvBackendOptions) -> Self {
        Self {
            source,
            timeout,
            options,
            state: None,
        }
    }

    fn state(&mut self) -> MediaResult<&mut DecvFrameExtractorState> {
        if self.state.is_none() {
            self.state = Some(DecvFrameExtractorState::open(
                &self.source,
                self.timeout,
                self.options,
            )?);
        }
        Ok(self
            .state
            .as_mut()
            .expect("decv frame extractor state was initialized"))
    }

    fn redact_error(&self, mut error: MediaError) -> MediaError {
        error.message = self.source.redact_error_message(&error.message).into();
        error
    }
}

impl FrameExtractionSession for DecvFrameExtractionSession {
    fn initial_frame(&mut self) -> MediaResult<Arc<VideoFrame>> {
        let result = self.state().and_then(|state| state.extract_initial_frame());
        result.map_err(|error| self.redact_error(error))
    }

    fn frame_at(
        &mut self,
        position: Duration,
        seek_mode: SeekMode,
    ) -> MediaResult<Arc<VideoFrame>> {
        let result = self.state().and_then(|state| {
            if position.is_zero() {
                state.extract_initial_frame()
            } else {
                state.extract_frame(position, Some(seek_mode))
            }
        });
        result.map_err(|error| self.redact_error(error))
    }
}

struct DecvFrameExtractorState {
    demuxer: DecvDemuxer,
    video_track_index: usize,
    duration: Option<Duration>,
    surface_handle: SurfaceHandle,
    sequence: u64,
    timeout: Duration,
    decoder: DecvVideoDecoder,
    seek_controller: Option<H264Mp4SeekController>,
    first_description: usize,
    interactive_seek_maximum_input_samples: usize,
    initial_frame: Option<Arc<VideoFrame>>,
}

impl DecvFrameExtractorState {
    fn open(
        source: &MediaSource,
        timeout: Duration,
        options: DecvBackendOptions,
    ) -> MediaResult<Self> {
        let prepared = probe_mp4(source)?;
        let (decoder_config, first_description) = {
            let cursor = prepared
                .demuxer
                .packet_cursor(prepared.probe.video_track_index)
                .map_err(|error| decv_container_error("failed to open MP4 packet cursor", error))?;
            let decoder_config = cursor
                .decoder_config()
                .map_err(|error| {
                    decv_container_error("failed to read MP4 decoder configuration", error)
                })?
                .ok_or_else(|| {
                    MediaError::new(
                        MediaErrorKind::UnsupportedCodec,
                        "MP4 video track has no decoder configuration",
                        MediaRecovery::None,
                    )
                })?;
            let first_description = cursor
                .track()
                .samples()
                .first()
                .ok_or_else(|| decv_container_message("MP4 video track has no samples"))?
                .description_index();
            (decoder_config, first_description)
        };
        let codec = decoder_config.codec;
        let decoder = DecvVideoDecoder::new(decoder_config, options.parallelism)?;
        Ok(Self {
            demuxer: prepared.demuxer,
            video_track_index: prepared.probe.video_track_index,
            duration: prepared.probe.duration,
            surface_handle: SurfaceHandle::new(),
            sequence: 0,
            timeout,
            decoder,
            seek_controller: (codec == VideoCodec::H264).then(|| {
                H264Mp4SeekController::new(
                    prepared.probe.video_track_index,
                    VIDEO_SEEK_CHECKPOINT_LIMIT,
                    VIDEO_SEEK_CHECKPOINT_REFERENCE_BYTES,
                )
            }),
            first_description,
            interactive_seek_maximum_input_samples: options.interactive_seek_maximum_input_samples,
            initial_frame: None,
        })
    }

    fn extract_frame(
        &mut self,
        requested: Duration,
        seek_mode: Option<SeekMode>,
    ) -> MediaResult<Arc<VideoFrame>> {
        let started = Instant::now();
        let mut cursor = self
            .demuxer
            .packet_cursor(self.video_track_index)
            .map_err(|error| decv_container_error("failed to open MP4 packet cursor", error))?;

        let mut minimum_pts = None;
        let mut discontinuity = false;
        if let Some(seek_mode) = seek_mode {
            let requested = self.duration.map_or(requested, |duration| {
                requested.min(duration.saturating_sub(Duration::from_millis(1)))
            });
            let target = duration_to_media_time(requested)?;
            let outcome = begin_video_seek(
                &mut self.decoder,
                &mut self.seek_controller,
                &mut cursor,
                target,
                seek_mode,
                false,
                self.interactive_seek_maximum_input_samples,
            )?;
            discontinuity = outcome.requires_discontinuity;
            minimum_pts = outcome.exact.then_some(requested);
        }

        let mut pending_packet = None;
        let mut draining = false;
        loop {
            if started.elapsed() >= self.timeout {
                return Err(MediaError::timeout(format!(
                    "timed out waiting for a decv video frame at {requested:?}"
                )));
            }
            match decode_video_step(
                &mut self.decoder,
                &mut cursor,
                &mut self.seek_controller,
                &mut pending_packet,
                &mut draining,
                &mut discontinuity,
                self.first_description,
            )? {
                VideoDecodeStep::Progress => {}
                VideoDecodeStep::Frame(frame) => {
                    let timestamp = media_time_to_duration(frame.pts);
                    if minimum_pts.is_some_and(|minimum| timestamp.is_some_and(|pts| pts < minimum))
                    {
                        continue;
                    }
                    self.sequence = self.sequence.wrapping_add(1).max(1);
                    return convert_frame(frame, self.surface_handle.clone(), self.sequence)
                        .map(Arc::new);
                }
                VideoDecodeStep::EndOfStream => {
                    return Err(decv_decode_message(format!(
                        "decv reached the end of the video before extracting a frame at {requested:?}"
                    )));
                }
            }
        }
    }

    fn extract_initial_frame(&mut self) -> MediaResult<Arc<VideoFrame>> {
        if let Some(frame) = &self.initial_frame {
            return Ok(frame.clone());
        }
        let frame = self.extract_frame(Duration::ZERO, None)?;
        self.initial_frame = Some(frame.clone());
        Ok(frame)
    }
}

#[derive(Debug)]
enum Command {
    Play,
    Pause,
    Reload {
        autoplay: bool,
        generation: u64,
    },
    Seek {
        position: Duration,
        mode: SeekMode,
        generation: u64,
    },
    SetVolume(f64),
    SetMuted(bool),
    Shutdown,
}

fn coalesce_seek_command(
    command: Command,
    receiver: &mpsc::Receiver<Command>,
    deferred_command: &mut Option<Command>,
) -> Command {
    if !matches!(command, Command::Seek { .. }) {
        return command;
    }

    let mut latest = command;
    loop {
        match receiver.try_recv() {
            Ok(command @ Command::Seek { .. }) => latest = command,
            Ok(command) => {
                *deferred_command = Some(command);
                break;
            }
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
        }
    }
    latest
}

fn seek_generation_is_current(handled: u64, latest: u64) -> bool {
    handled == latest
}

fn network_prefetch_is_due(
    last_request: Option<(u64, u64)>,
    generation: u64,
    sample_offset: u64,
) -> bool {
    !last_request.is_some_and(|(requested_generation, requested_offset)| {
        requested_generation == generation
            && sample_offset >= requested_offset
            && sample_offset - requested_offset < HTTP_PREFETCH_REARM_BYTES
    })
}

#[derive(Clone, Copy, Debug)]
struct NetworkPrefetchRequest {
    track_index: usize,
    first_sample_index: usize,
    generation: u64,
}

struct NetworkPrefetcher {
    requests: Option<mpsc::SyncSender<NetworkPrefetchRequest>>,
    shutdown: Arc<AtomicBool>,
    last_request: Option<(u64, u64)>,
    worker: Option<JoinHandle<()>>,
}

impl NetworkPrefetcher {
    fn start(demuxer: &DecvDemuxer, latest_seek_generation: Arc<AtomicU64>) -> MediaResult<Self> {
        let worker_demuxer = Mp4Demuxer::open(demuxer.input().clone()).map_err(|error| {
            decv_container_error("failed to prepare MP4 network prefetch", error)
        })?;
        let (requests, receiver) = mpsc::sync_channel(1);
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = shutdown.clone();
        let worker = thread::Builder::new()
            .name("gpui-video-decv-io".to_owned())
            .spawn(move || {
                network_prefetch_worker(
                    worker_demuxer,
                    receiver,
                    worker_shutdown,
                    latest_seek_generation,
                )
            })
            .map_err(|error| {
                MediaError::io("failed to start decv network prefetch thread", error)
            })?;
        Ok(Self {
            requests: Some(requests),
            shutdown,
            last_request: None,
            worker: Some(worker),
        })
    }

    fn request_now(&mut self, cursor: &PacketCursor<'_, DecvMediaInput>, generation: u64) {
        self.last_request = None;
        self.request_if_needed(cursor, generation);
    }

    fn mark_current_as_prefetched(
        &mut self,
        cursor: &PacketCursor<'_, DecvMediaInput>,
        generation: u64,
    ) {
        self.last_request = cursor
            .track()
            .samples()
            .get(cursor.next_sample_index())
            .map(|sample| (generation, sample.offset()));
    }

    fn request_if_needed(&mut self, cursor: &PacketCursor<'_, DecvMediaInput>, generation: u64) {
        let first_sample_index = cursor.next_sample_index();
        let Some(sample) = cursor.track().samples().get(first_sample_index) else {
            return;
        };
        let sample_offset = sample.offset();
        if !network_prefetch_is_due(self.last_request, generation, sample_offset) {
            return;
        }

        let request = NetworkPrefetchRequest {
            track_index: cursor.track_index(),
            first_sample_index,
            generation,
        };
        let Some(requests) = self.requests.as_ref() else {
            return;
        };
        match requests.try_send(request) {
            Ok(()) => self.last_request = Some((generation, sample_offset)),
            Err(mpsc::TrySendError::Full(_)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.requests = None;
            }
        }
    }
}

impl Drop for NetworkPrefetcher {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.requests.take();
        // A blocking Range request cannot be interrupted through MediaInput.
        // Detach and let the I/O worker notice shutdown after that request.
        self.worker.take();
    }
}

fn network_prefetch_worker(
    demuxer: DecvDemuxer,
    requests: mpsc::Receiver<NetworkPrefetchRequest>,
    shutdown: Arc<AtomicBool>,
    latest_seek_generation: Arc<AtomicU64>,
) {
    while !shutdown.load(Ordering::Acquire) {
        let Ok(mut request) = requests.recv() else {
            break;
        };
        while let Ok(newer) = requests.try_recv() {
            request = newer;
        }
        if shutdown.load(Ordering::Acquire)
            || !seek_generation_is_current(
                request.generation,
                latest_seek_generation.load(Ordering::Acquire),
            )
        {
            continue;
        }
        if let Err(error) = demuxer.prefetch_track_bytes(
            request.track_index,
            request.first_sample_index,
            HTTP_PREFETCH_WINDOW_BYTES,
        ) && seek_generation_is_current(
            request.generation,
            latest_seek_generation.load(Ordering::Acquire),
        ) {
            log::warn!(
                target: "gpui_media::decv_network",
                "decv network prefetch failed at video sample {}: {error}",
                request.first_sample_index,
            );
        }
    }
}

struct SeekPlaybackState {
    handled_generation: u64,
    interactive_preroll: bool,
    interactive_exact_target: Option<Duration>,
    reusable_interactive_exact_target: Option<Duration>,
    ready_without_frame: Option<(Duration, u64)>,
}

impl SeekPlaybackState {
    fn new(handled_generation: u64) -> Self {
        Self {
            handled_generation,
            interactive_preroll: false,
            interactive_exact_target: None,
            reusable_interactive_exact_target: None,
            ready_without_frame: None,
        }
    }

    fn reset_interactive(&mut self) {
        self.interactive_preroll = false;
        self.interactive_exact_target = None;
        self.reusable_interactive_exact_target = None;
        self.ready_without_frame = None;
    }

    fn begin_interactive_preroll(&mut self, exact_target: Option<Duration>) {
        self.interactive_preroll = true;
        self.interactive_exact_target = exact_target;
        self.reusable_interactive_exact_target = None;
        self.ready_without_frame = None;
    }

    fn complete_interactive_preroll(&mut self) {
        self.interactive_preroll = false;
        self.reusable_interactive_exact_target = self.interactive_exact_target.take();
    }

    fn can_reuse_interactive_exact(&self, target: Duration) -> bool {
        self.reusable_interactive_exact_target == Some(target)
    }
}

struct Probe {
    video_track_index: usize,
    audio_track_index: Option<usize>,
    duration: Option<Duration>,
}

struct PreparedMedia {
    demuxer: DecvDemuxer,
    probe: Probe,
    network: bool,
}

fn decv_container_error(
    context: impl std::fmt::Display,
    error: impl std::fmt::Debug,
) -> MediaError {
    MediaError::new(
        MediaErrorKind::UnsupportedContainer,
        format!("{context}: {error:?}"),
        MediaRecovery::None,
    )
}

fn decv_container_message(message: impl Into<gpui::SharedString>) -> MediaError {
    MediaError::new(
        MediaErrorKind::UnsupportedContainer,
        message,
        MediaRecovery::None,
    )
}

fn decv_decode_error(context: impl std::fmt::Display, error: impl std::fmt::Debug) -> MediaError {
    MediaError::new(
        MediaErrorKind::Decode,
        format!("{context}: {error:?}"),
        MediaRecovery::None,
    )
}

fn decv_decode_message(message: impl Into<gpui::SharedString>) -> MediaError {
    MediaError::new(MediaErrorKind::Decode, message, MediaRecovery::None)
}

fn decv_video_output_error(
    context: impl std::fmt::Display,
    error: impl std::fmt::Debug,
) -> MediaError {
    MediaError::new(
        MediaErrorKind::VideoOutput,
        format!("{context}: {error:?}"),
        MediaRecovery::None,
    )
}

fn decv_video_output_message(message: impl Into<gpui::SharedString>) -> MediaError {
    MediaError::new(MediaErrorKind::VideoOutput, message, MediaRecovery::None)
}

fn probe_mp4(source: &MediaSource) -> MediaResult<PreparedMedia> {
    let network = matches!(
        url::Url::parse(source.uri())
            .ok()
            .as_ref()
            .map(url::Url::scheme),
        Some("http" | "https")
    );
    let demuxer = Mp4Demuxer::open(open_media_input(source)?).map_err(|error| {
        decv_container_error(
            format!("failed to parse MP4 {}", source.display_name()),
            error,
        )
    })?;
    let video_track_index = video_track_index(&demuxer)?;
    let audio_track_index = audio_track_index(&demuxer);
    let declared_duration = demuxer
        .movie()
        .duration()
        .map(|value| duration_from_ratio(value, demuxer.movie().timescale().get()))
        .transpose()?;
    let video_duration = track_presentation_duration(&demuxer.movie().tracks()[video_track_index])?;
    let audio_duration = audio_track_index
        .map(|track_index| track_presentation_duration(&demuxer.movie().tracks()[track_index]))
        .transpose()?
        .flatten();
    let duration = [declared_duration, video_duration, audio_duration]
        .into_iter()
        .flatten()
        .max();
    Ok(PreparedMedia {
        demuxer,
        probe: Probe {
            video_track_index,
            audio_track_index,
            duration,
        },
        network,
    })
}

fn playback_worker(
    demuxer: DecvDemuxer,
    video_track_index: usize,
    audio_track_index: Option<usize>,
    network: bool,
    commands: mpsc::Receiver<Command>,
    shared_timeline: &Mutex<TimelineState>,
    latest_seek_generation: Arc<AtomicU64>,
    output: &MediaOutputSink,
    parallelism: DecvParallelism,
    interactive_seek_maximum_input_samples: usize,
) -> MediaResult<()> {
    if video_track_index >= demuxer.movie().tracks().len() {
        return Err(decv_container_message(
            "video track changed after probing the MP4",
        ));
    }
    let mut cursor = demuxer
        .packet_cursor(video_track_index)
        .map_err(|error| decv_container_error("failed to open MP4 packet cursor", error))?;
    let decoder_config = cursor
        .decoder_config()
        .map_err(|error| decv_container_error("failed to read MP4 decoder configuration", error))?
        .ok_or_else(|| {
            MediaError::new(
                MediaErrorKind::UnsupportedCodec,
                "MP4 video track has no decoder configuration",
                MediaRecovery::None,
            )
        })?;
    let first_description = cursor
        .track()
        .samples()
        .first()
        .ok_or_else(|| decv_container_message("MP4 video track has no samples"))?
        .description_index();

    let codec = decoder_config.codec;
    let mut decoder = DecvVideoDecoder::new(decoder_config, parallelism)?;
    let mut seek_controller = (codec == VideoCodec::H264).then(|| {
        H264Mp4SeekController::new(
            video_track_index,
            VIDEO_SEEK_CHECKPOINT_LIMIT,
            VIDEO_SEEK_CHECKPOINT_REFERENCE_BYTES,
        )
    });
    log::debug!("decv configured {codec:?} video decoder");
    let mut audio = audio_track_index
        .map(|track_index| DecvAudioPlayback::open(&demuxer, track_index))
        .transpose()?;
    shared_timeline.lock_unpoisoned().audio_clock = audio.as_ref().map(DecvAudioPlayback::clock);
    let initial_prefetch_complete = if network {
        match cursor.prefetch_next_bytes(HTTP_PREFETCH_WINDOW_BYTES) {
            Ok(covered_samples) => covered_samples != 0,
            Err(error) => {
                log::warn!(
                    target: "gpui_media::decv_network",
                    "decv initial network prefetch failed: {error}",
                );
                false
            }
        }
    } else {
        false
    };
    let mut network_prefetch = network
        .then(|| NetworkPrefetcher::start(&demuxer, latest_seek_generation.clone()))
        .transpose()?;

    let surface_handle = SurfaceHandle::new();
    let mut sequence = 0u64;
    let mut pending_packet = None;
    let mut pending_frames = VecDeque::with_capacity(VIDEO_DECODE_QUEUE_CAPACITY);
    let mut draining = false;
    let mut video_decode_complete = false;
    let mut discontinuity = false;
    let mut playing = false;
    let mut preroll = false;
    let mut ended = false;
    let mut minimum_pts = None;
    let mut seek_state = SeekPlaybackState::new(latest_seek_generation.load(Ordering::Acquire));
    let mut deferred_command = None;
    let mut clock = PresentationClock::new(Duration::ZERO);
    let mut seek_preroll_skipped_frames = 0u64;
    let mut stale_seek_skipped_frames = 0u64;
    if let Some(prefetch) = network_prefetch.as_mut() {
        if initial_prefetch_complete {
            prefetch.mark_current_as_prefetched(&cursor, seek_state.handled_generation);
        } else {
            prefetch.request_now(&cursor, seek_state.handled_generation);
        }
    }

    loop {
        if let Some(prefetch) = network_prefetch.as_mut() {
            prefetch.request_if_needed(&cursor, seek_state.handled_generation);
        }
        if let Some((position, generation)) = seek_state.ready_without_frame.take() {
            if seek_generation_is_current(
                generation,
                latest_seek_generation.load(Ordering::Acquire),
            ) {
                let audio_clock = audio
                    .as_ref()
                    .filter(|audio| audio.uses_device_clock())
                    .map(DecvAudioPlayback::clock);
                clock = PresentationClock::new(position);
                shared_timeline.lock_unpoisoned().complete_seek(
                    position,
                    playing,
                    audio_clock,
                    Instant::now(),
                );
                if playing && let Some(audio) = audio.as_ref() {
                    audio.play()?;
                }
                output.emit(MediaBackendEvent::Ready);
            }
            continue;
        }

        if !playing && !preroll {
            let command = match deferred_command.take() {
                Some(command) => command,
                None => commands
                    .recv()
                    .map_err(|_| MediaError::backend("decv playback command channel closed"))?,
            };
            let command = coalesce_seek_command(command, &commands, &mut deferred_command);
            if apply_command(
                command,
                &mut cursor,
                &mut decoder,
                &mut seek_controller,
                &mut seek_state,
                interactive_seek_maximum_input_samples,
                &mut pending_packet,
                &mut pending_frames,
                &mut draining,
                &mut video_decode_complete,
                &mut discontinuity,
                &mut audio,
                &mut playing,
                &mut preroll,
                &mut ended,
                &mut minimum_pts,
                &mut clock,
                shared_timeline,
            )? {
                return Ok(());
            }
            continue;
        }

        let command = deferred_command.take().or_else(|| commands.try_recv().ok());
        if let Some(command) = command {
            let command = coalesce_seek_command(command, &commands, &mut deferred_command);
            if apply_command(
                command,
                &mut cursor,
                &mut decoder,
                &mut seek_controller,
                &mut seek_state,
                interactive_seek_maximum_input_samples,
                &mut pending_packet,
                &mut pending_frames,
                &mut draining,
                &mut video_decode_complete,
                &mut discontinuity,
                &mut audio,
                &mut playing,
                &mut preroll,
                &mut ended,
                &mut minimum_pts,
                &mut clock,
                shared_timeline,
            )? {
                return Ok(());
            }
            continue;
        }

        if let Some(audio) = audio.as_mut() {
            audio.pump(if preroll {
                AUDIO_PREROLL_TARGET
            } else {
                AUDIO_BUFFER_TARGET
            })?;
            if !preroll {
                if let Some(position) = audio.release_device_clock_if_complete() {
                    clock = PresentationClock::new(position);
                    shared_timeline
                        .lock_unpoisoned()
                        .release_audio_clock(position, Instant::now());
                } else if playing {
                    shared_timeline
                        .lock_unpoisoned()
                        .present(audio.position(), Instant::now());
                }
            }
        }
        if pending_frames.is_empty() && !video_decode_complete {
            let decode_step = decode_video_step(
                &mut decoder,
                &mut cursor,
                &mut seek_controller,
                &mut pending_packet,
                &mut draining,
                &mut discontinuity,
                first_description,
            )?;
            match decode_step {
                VideoDecodeStep::Progress => continue,
                VideoDecodeStep::Frame(frame) => {
                    pending_frames.push_back(frame);
                }
                VideoDecodeStep::EndOfStream => {
                    video_decode_complete = true;
                }
            }
            let command = deferred_command.take().or_else(|| commands.try_recv().ok());
            if let Some(command) = command {
                let command = coalesce_seek_command(command, &commands, &mut deferred_command);
                if apply_command(
                    command,
                    &mut cursor,
                    &mut decoder,
                    &mut seek_controller,
                    &mut seek_state,
                    interactive_seek_maximum_input_samples,
                    &mut pending_packet,
                    &mut pending_frames,
                    &mut draining,
                    &mut video_decode_complete,
                    &mut discontinuity,
                    &mut audio,
                    &mut playing,
                    &mut preroll,
                    &mut ended,
                    &mut minimum_pts,
                    &mut clock,
                    shared_timeline,
                )? {
                    return Ok(());
                }
                continue;
            }
        }

        if pending_frames.is_empty() && video_decode_complete {
            let audio_pending = audio
                .as_ref()
                .is_some_and(|audio| !audio.is_decode_complete() || !audio.is_drained());
            if audio_pending {
                match commands.recv_timeout(AUDIO_CLOCK_POLL_INTERVAL) {
                    Ok(command) => {
                        let command =
                            coalesce_seek_command(command, &commands, &mut deferred_command);
                        if apply_command(
                            command,
                            &mut cursor,
                            &mut decoder,
                            &mut seek_controller,
                            &mut seek_state,
                            interactive_seek_maximum_input_samples,
                            &mut pending_packet,
                            &mut pending_frames,
                            &mut draining,
                            &mut video_decode_complete,
                            &mut discontinuity,
                            &mut audio,
                            &mut playing,
                            &mut preroll,
                            &mut ended,
                            &mut minimum_pts,
                            &mut clock,
                            shared_timeline,
                        )? {
                            return Ok(());
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                continue;
            }
            if !ended {
                ended = true;
                if let Some(audio) = audio.as_ref() {
                    audio.pause()?;
                }
                playing = false;
                let now = Instant::now();
                shared_timeline.lock_unpoisoned().finish(now);
                output.emit(MediaBackendEvent::Ended);
            }
            continue;
        }

        let frame = pending_frames
            .front()
            .expect("the decoder produced a pending frame");
        let timestamp = media_time_to_duration(frame.pts);
        if minimum_pts.is_some_and(|minimum| timestamp.is_some_and(|pts| pts < minimum)) {
            seek_preroll_skipped_frames = seek_preroll_skipped_frames.saturating_add(1);
            log::debug!(
                target: "gpui_media::decv_frame",
                "decv seek-preroll-skip: count={seek_preroll_skipped_frames} \
                 frame_pts={timestamp:?} minimum_pts={minimum_pts:?}",
            );
            pending_frames.pop_front();
            continue;
        }
        minimum_pts = None;

        let timestamp = timestamp.unwrap_or_else(|| {
            shared_timeline
                .lock_unpoisoned()
                .snapshot(Instant::now())
                .position()
        });
        let audio_wait = audio.as_ref().and_then(|audio| {
            audio_clock_wait(
                timestamp,
                audio.position(),
                preroll,
                audio.uses_device_clock(),
            )
        });
        let wait = if let Some(audio_wait) = audio_wait {
            audio_wait.min(AUDIO_CLOCK_POLL_INTERVAL)
        } else if preroll {
            Duration::ZERO
        } else {
            clock
                .deadline(timestamp)
                .saturating_duration_since(Instant::now())
        };
        if !wait.is_zero()
            && !preroll
            && !video_decode_complete
            && pending_frames.len() < VIDEO_DECODE_QUEUE_CAPACITY
        {
            match decode_video_step(
                &mut decoder,
                &mut cursor,
                &mut seek_controller,
                &mut pending_packet,
                &mut draining,
                &mut discontinuity,
                first_description,
            )? {
                VideoDecodeStep::Progress => continue,
                VideoDecodeStep::Frame(frame) => {
                    pending_frames.push_back(frame);
                    continue;
                }
                VideoDecodeStep::EndOfStream => {
                    video_decode_complete = true;
                    continue;
                }
            }
        }
        if !wait.is_zero() {
            match commands.recv_timeout(wait) {
                Ok(command) => {
                    let command = coalesce_seek_command(command, &commands, &mut deferred_command);
                    if apply_command(
                        command,
                        &mut cursor,
                        &mut decoder,
                        &mut seek_controller,
                        &mut seek_state,
                        interactive_seek_maximum_input_samples,
                        &mut pending_packet,
                        &mut pending_frames,
                        &mut draining,
                        &mut video_decode_complete,
                        &mut discontinuity,
                        &mut audio,
                        &mut playing,
                        &mut preroll,
                        &mut ended,
                        &mut minimum_pts,
                        &mut clock,
                        shared_timeline,
                    )? {
                        return Ok(());
                    }
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if audio_wait.is_some() {
                        continue;
                    }
                }
            }
        }

        let frame = pending_frames
            .pop_front()
            .expect("the scheduled frame remains pending");
        let completed_preroll = preroll;
        let completed_interactive_preroll = seek_state.interactive_preroll;
        if !seek_generation_is_current(
            seek_state.handled_generation,
            latest_seek_generation.load(Ordering::Acquire),
        ) {
            stale_seek_skipped_frames = stale_seek_skipped_frames.saturating_add(1);
            log::debug!(
                target: "gpui_media::decv_frame",
                "decv stale-seek-frame-skip: count={stale_seek_skipped_frames} \
                 frame_pts={:?} handled_generation={} latest_generation={}",
                media_time_to_duration(frame.pts),
                seek_state.handled_generation,
                latest_seek_generation.load(Ordering::Acquire),
            );
            continue;
        }
        sequence = sequence.wrapping_add(1);
        let frame = Arc::new(convert_frame(frame, surface_handle.clone(), sequence)?);
        let timestamp = frame.timestamp().unwrap_or(timestamp);
        output.publish_video_frame(frame);
        preroll = false;
        if completed_interactive_preroll {
            seek_state.complete_interactive_preroll();
            playing = false;
        } else if completed_preroll {
            seek_state.reusable_interactive_exact_target = None;
            let audio_clock = audio
                .as_ref()
                .filter(|audio| audio.uses_device_clock())
                .map(DecvAudioPlayback::clock);
            let timeline_position = audio
                .as_ref()
                .filter(|audio| playing && audio.uses_device_clock())
                .map_or(timestamp, DecvAudioPlayback::position);
            clock = PresentationClock::new(timeline_position);
            shared_timeline.lock_unpoisoned().complete_seek(
                timeline_position,
                playing,
                audio_clock,
                Instant::now(),
            );
            if playing && let Some(audio) = audio.as_ref() {
                audio.play()?;
            }
            output.emit(MediaBackendEvent::Ready);
        } else {
            seek_state.reusable_interactive_exact_target = None;
            let timeline_position = audio
                .as_ref()
                .filter(|audio| playing && audio.uses_device_clock())
                .map_or(timestamp, DecvAudioPlayback::position);
            shared_timeline
                .lock_unpoisoned()
                .present(timeline_position, Instant::now());
        }
        if output.is_closed() {
            return Ok(());
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct VideoSeekOutcome {
    seek_time: MediaTime,
    requires_discontinuity: bool,
    exact: bool,
    interactive: bool,
}

fn begin_video_seek(
    decoder: &mut DecvVideoDecoder,
    seek_controller: &mut Option<H264Mp4SeekController>,
    cursor: &mut PacketCursor<'_, DecvMediaInput>,
    target: MediaTime,
    mode: SeekMode,
    allow_forward_retarget: bool,
    interactive_seek_maximum_input_samples: usize,
) -> MediaResult<VideoSeekOutcome> {
    if decoder.codec() == VideoCodec::H264 {
        let seek_controller = seek_controller
            .as_mut()
            .ok_or_else(|| decv_decode_message("decv H.264 decoder has no MP4 seek controller"))?;
        let decoder = decoder
            .h264_mut()
            .expect("the checked decv decoder is H.264");
        return match mode {
            SeekMode::Accurate => {
                let outcome = seek_controller
                    .begin_exact_seek(decoder, cursor, target, allow_forward_retarget)
                    .map_err(|error| {
                        decv_decode_error("failed to coordinate exact H.264 MP4 seek", error)
                    })?;
                Ok(VideoSeekOutcome {
                    seek_time: target,
                    requires_discontinuity: outcome.requires_discontinuity(),
                    exact: true,
                    interactive: false,
                })
            }
            SeekMode::Interactive => {
                let decision = seek_controller
                    .begin_interactive_seek(
                        decoder,
                        cursor,
                        target,
                        allow_forward_retarget,
                        interactive_seek_maximum_input_samples,
                    )
                    .map_err(|error| {
                        decv_decode_error("failed to coordinate interactive H.264 MP4 seek", error)
                    })?;
                let seek_time = if decision.is_exact() {
                    target
                } else {
                    let sample_index = decision.preview_sample_index().ok_or_else(|| {
                        decv_decode_message(
                            "interactive decv seek returned neither an exact nor preview result",
                        )
                    })?;
                    video_sample_time(cursor, sample_index)?
                };
                Ok(VideoSeekOutcome {
                    seek_time,
                    requires_discontinuity: decision.requires_discontinuity(),
                    exact: decision.is_exact(),
                    interactive: true,
                })
            }
            SeekMode::KeyFrame => {
                let sample_index = seek_controller
                    .begin_nearest_preview(decoder, cursor, target)
                    .map_err(|error| {
                        decv_decode_error("failed to coordinate H.264 MP4 seek preview", error)
                    })?;
                Ok(VideoSeekOutcome {
                    seek_time: video_sample_time(cursor, sample_index)?,
                    requires_discontinuity: true,
                    exact: false,
                    interactive: false,
                })
            }
        };
    }

    decoder.flush();
    let (sample_index, exact, interactive) = match mode {
        SeekMode::Accurate => (
            cursor
                .seek_to_keyframe(target)
                .map_err(|error| decv_container_error("failed to seek VP9 MP4", error))?,
            true,
            false,
        ),
        SeekMode::Interactive => (
            cursor
                .seek_to_nearest_keyframe(target)
                .map_err(|error| decv_container_error("failed to seek VP9 MP4 preview", error))?,
            false,
            true,
        ),
        SeekMode::KeyFrame => (
            cursor.seek_to_nearest_keyframe(target).map_err(|error| {
                decv_container_error("failed to seek VP9 MP4 keyframe preview", error)
            })?,
            false,
            false,
        ),
    };
    let sample_index =
        sample_index.ok_or_else(|| decv_decode_message("MP4 has no VP9 keyframe for seek"))?;
    Ok(VideoSeekOutcome {
        seek_time: if exact {
            target
        } else {
            video_sample_time(cursor, sample_index)?
        },
        requires_discontinuity: true,
        exact,
        interactive,
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_command(
    command: Command,
    cursor: &mut PacketCursor<'_, DecvMediaInput>,
    decoder: &mut DecvVideoDecoder,
    seek_controller: &mut Option<H264Mp4SeekController>,
    seek_state: &mut SeekPlaybackState,
    interactive_seek_maximum_input_samples: usize,
    pending_packet: &mut Option<EncodedVideoPacket>,
    pending_frames: &mut VecDeque<DecodedVideoFrame>,
    draining: &mut bool,
    video_decode_complete: &mut bool,
    discontinuity: &mut bool,
    audio: &mut Option<DecvAudioPlayback<'_>>,
    playing: &mut bool,
    preroll: &mut bool,
    ended: &mut bool,
    minimum_pts: &mut Option<Duration>,
    clock: &mut PresentationClock,
    shared_timeline: &Mutex<TimelineState>,
) -> MediaResult<bool> {
    match command {
        Command::Play => {
            let position = shared_timeline
                .lock_unpoisoned()
                .snapshot(Instant::now())
                .position();
            *clock = PresentationClock::new(position);
            if seek_state.interactive_preroll
                || seek_state.reusable_interactive_exact_target.is_some()
            {
                *playing = false;
            } else {
                if !*preroll && let Some(audio) = audio {
                    audio.play()?;
                }
                *playing = true;
            }
        }
        Command::Pause => {
            if let Some(audio) = audio {
                audio.pause()?;
            }
            *playing = false;
            *preroll = seek_state.interactive_preroll
                || (pending_frames.is_empty() && cursor.next_sample_index() == 0 && !*ended);
        }
        Command::Reload {
            autoplay,
            generation,
        } => {
            cursor.rewind();
            reset_decode_timeline(
                decoder,
                pending_packet,
                pending_frames,
                draining,
                video_decode_complete,
            );
            if let Some(seek_controller) = seek_controller {
                seek_controller.clear();
            }
            seek_state.reset_interactive();
            seek_state.handled_generation = generation;
            *discontinuity = false;
            if let Some(audio) = audio {
                audio.reload(false)?;
            }
            *minimum_pts = None;
            *ended = false;
            *playing = autoplay;
            *preroll = true;
            *clock = PresentationClock::new(Duration::ZERO);
        }
        Command::Seek {
            mut position,
            mode,
            generation,
        } => {
            if let Some(duration) = shared_timeline.lock_unpoisoned().duration {
                position = position.min(duration);
            }
            if seek_state.can_reuse_interactive_exact(position)
                && matches!(mode, SeekMode::Accurate | SeekMode::Interactive)
            {
                seek_state.handled_generation = generation;
                seek_state.interactive_preroll = false;
                seek_state.interactive_exact_target = None;
                *minimum_pts = None;
                *ended = false;
                *clock = PresentationClock::new(position);
                *preroll = false;
                if mode == SeekMode::Accurate {
                    *playing = shared_timeline.lock_unpoisoned().playing;
                    seek_state.reusable_interactive_exact_target = None;
                    seek_state.ready_without_frame = Some((position, generation));
                } else {
                    *playing = false;
                }
                return Ok(false);
            }

            let target = duration_to_media_time(position)?;
            let allow_forward_retarget = *preroll && !*draining && pending_packet.is_none();
            let outcome = begin_video_seek(
                decoder,
                seek_controller,
                cursor,
                target,
                mode,
                allow_forward_retarget,
                interactive_seek_maximum_input_samples,
            )?;
            let VideoSeekOutcome {
                seek_time,
                requires_discontinuity,
                exact,
                interactive,
            } = outcome;
            let seek_position = media_time_to_duration(Some(seek_time)).unwrap_or(Duration::ZERO);
            let timeline_position = if interactive { position } else { seek_position };
            *pending_packet = None;
            pending_frames.clear();
            *draining = false;
            *video_decode_complete = false;
            *discontinuity = requires_discontinuity;
            *minimum_pts = exact.then_some(position);
            *ended = false;
            *clock = PresentationClock::new(timeline_position);
            let resume_playing = shared_timeline.lock_unpoisoned().playing;
            *playing = if interactive { false } else { resume_playing };
            shared_timeline.lock_unpoisoned().begin_seek(
                timeline_position,
                resume_playing,
                Instant::now(),
            );
            if let Some(audio) = audio {
                let audio_seek_time = if interactive { target } else { seek_time };
                audio.seek(audio_seek_time, timeline_position)?;
            }
            seek_state.handled_generation = generation;
            if interactive {
                seek_state.begin_interactive_preroll(exact.then_some(position));
            } else {
                seek_state.reset_interactive();
            }
            *preroll = true;
        }
        Command::SetVolume(volume) => {
            if let Some(audio) = audio {
                audio.set_volume(volume);
            }
        }
        Command::SetMuted(muted) => {
            if let Some(audio) = audio {
                audio.set_muted(muted);
            }
        }
        Command::Shutdown => return Ok(true),
    }
    Ok(false)
}

fn reset_decode_timeline(
    decoder: &mut DecvVideoDecoder,
    pending_packet: &mut Option<EncodedVideoPacket>,
    pending_frames: &mut VecDeque<DecodedVideoFrame>,
    draining: &mut bool,
    video_decode_complete: &mut bool,
) {
    decoder.flush();
    *pending_packet = None;
    pending_frames.clear();
    *draining = false;
    *video_decode_complete = false;
}

enum VideoDecodeStep {
    Progress,
    Frame(DecodedVideoFrame),
    EndOfStream,
}

fn decode_video_step(
    decoder: &mut DecvVideoDecoder,
    cursor: &mut PacketCursor<'_, DecvMediaInput>,
    seek_controller: &mut Option<H264Mp4SeekController>,
    pending_packet: &mut Option<EncodedVideoPacket>,
    draining: &mut bool,
    discontinuity: &mut bool,
    first_description: usize,
) -> MediaResult<VideoDecodeStep> {
    match decoder.receive_frame()? {
        DecodeOutput::Frame(frame) => Ok(VideoDecodeStep::Frame(frame)),
        DecodeOutput::FormatChanged(format) => {
            log::debug!(
                "decv format changed: {}x{} {:?}",
                format.coded_size.width,
                format.coded_size.height,
                format.pixel_format
            );
            Ok(VideoDecodeStep::Progress)
        }
        DecodeOutput::EndOfStream => Ok(VideoDecodeStep::EndOfStream),
        DecodeOutput::NeedInput => {
            if *draining {
                return Err(decv_decode_message("decv requested input after drain"));
            }

            let packet = match pending_packet.take() {
                Some(packet) => packet,
                None => {
                    let Some(mut packet) = cursor.next_packet().map_err(|error| {
                        decv_container_error("failed to read the next MP4 packet", error)
                    })?
                    else {
                        decoder.drain()?;
                        *draining = true;
                        return Ok(VideoDecodeStep::Progress);
                    };
                    let sample = &cursor.track().samples()[cursor.next_sample_index() - 1];
                    if sample.description_index() != first_description {
                        return Err(MediaError::new(
                            MediaErrorKind::UnsupportedCodec,
                            "mid-stream MP4 sample-description changes are not supported",
                            MediaRecovery::None,
                        ));
                    }
                    if std::mem::take(discontinuity) {
                        packet.discontinuity = true;
                    }
                    packet
                }
            };

            match decoder.send_packet(packet)? {
                DecodeInputStatus::Accepted => {
                    let next_sample = cursor.next_sample_index();
                    if let (Some(seek_controller), Some(decoder)) =
                        (seek_controller.as_mut(), decoder.h264_mut())
                        && let Err(error) = seek_controller.capture_checkpoint(decoder, cursor)
                    {
                        log::debug!(
                            "decv skipped H.264 seek checkpoint at MP4 sample {next_sample}: {error}"
                        );
                    }
                }
                DecodeInputStatus::NeedOutput(unconsumed) => {
                    *pending_packet = Some(unconsumed);
                }
                status => {
                    return Err(decv_decode_message(format!(
                        "decv returned an unsupported input status: {status:?}"
                    )));
                }
            }
            Ok(VideoDecodeStep::Progress)
        }
        output => Err(decv_decode_message(format!(
            "decv returned an unsupported decoder output: {output:?}"
        ))),
    }
}

struct DecvAudioPlayback<'demuxer> {
    cursor: AudioPacketCursor<'demuxer, DecvMediaInput>,
    decoder: SoftwareAudioDecoder,
    pending_packet: Option<EncodedAudioPacket>,
    first_description: usize,
    draining: bool,
    decode_complete: bool,
    minimum_pts: Option<Duration>,
    output: AudioOutput,
    device_clock_released: bool,
}

impl<'demuxer> DecvAudioPlayback<'demuxer> {
    fn open(demuxer: &'demuxer DecvDemuxer, track_index: usize) -> MediaResult<Self> {
        let cursor = demuxer.audio_packet_cursor(track_index).map_err(|error| {
            decv_container_error("failed to open MP4 audio packet cursor", error)
        })?;
        let config = cursor
            .decoder_config()
            .map_err(|error| {
                decv_container_error("failed to read MP4 audio decoder configuration", error)
            })?
            .ok_or_else(|| {
                MediaError::new(
                    MediaErrorKind::UnsupportedCodec,
                    "MP4 audio track has no decoder configuration",
                    MediaRecovery::None,
                )
            })?;
        let first_description = cursor
            .track()
            .samples()
            .first()
            .ok_or_else(|| decv_container_message("MP4 audio track has no samples"))?
            .description_index();
        let format = AudioFormat::new(config.sample_rate, config.channel_layout);
        let mut decoder = SoftwareAudioDecoder::new();
        decoder.configure(config).map_err(|error| {
            MediaError::new(
                MediaErrorKind::UnsupportedCodec,
                format!("failed to configure decv AAC decoder: {error}"),
                MediaRecovery::None,
            )
        })?;
        let mut output = AudioOutput::open(format)?;
        output.reset(Duration::ZERO);
        Ok(Self {
            cursor,
            decoder,
            pending_packet: None,
            first_description,
            draining: false,
            decode_complete: false,
            minimum_pts: Some(Duration::ZERO),
            output,
            device_clock_released: false,
        })
    }

    fn pump(&mut self, target: Duration) -> MediaResult<()> {
        self.output.check_error()?;
        while self.output.buffered_duration() < target && !self.decode_complete {
            match self.decoder.receive_frame().map_err(|error| {
                decv_decode_error("decv failed while receiving decoded audio", error)
            })? {
                AudioDecodeOutput::Frame(frame) => {
                    self.output.enqueue(frame, self.minimum_pts)?;
                }
                AudioDecodeOutput::FormatChanged(format) => {
                    log::debug!(
                        "decv audio format changed: {} Hz, {} channels, {:?}",
                        format.sample_rate,
                        format.channel_layout.channels(),
                        format.sample_format
                    );
                }
                AudioDecodeOutput::EndOfStream => {
                    self.output.flush_resampler();
                    self.decode_complete = true;
                }
                AudioDecodeOutput::NeedInput => {
                    if self.draining {
                        return Err(decv_decode_message(
                            "decv AAC decoder requested input after drain",
                        ));
                    }
                    let packet = match self.pending_packet.take() {
                        Some(packet) => packet,
                        None => {
                            let Some(packet) = self.cursor.next_packet().map_err(|error| {
                                decv_container_error(
                                    "failed to read the next MP4 audio packet",
                                    error,
                                )
                            })?
                            else {
                                self.decoder.drain().map_err(|error| {
                                    decv_decode_error("decv failed to drain AAC audio", error)
                                })?;
                                self.draining = true;
                                continue;
                            };
                            let sample =
                                &self.cursor.track().samples()[self.cursor.next_sample_index() - 1];
                            if sample.description_index() != self.first_description {
                                return Err(MediaError::new(
                                    MediaErrorKind::UnsupportedCodec,
                                    "mid-stream MP4 audio sample-description changes are not supported",
                                    MediaRecovery::None,
                                ));
                            }
                            packet
                        }
                    };
                    match self.decoder.send_packet(packet).map_err(|error| {
                        decv_decode_error("decv failed while decoding an AAC packet", error)
                    })? {
                        AudioDecodeInputStatus::Accepted => {}
                        AudioDecodeInputStatus::NeedOutput(unconsumed) => {
                            self.pending_packet = Some(unconsumed);
                        }
                        status => {
                            return Err(decv_decode_message(format!(
                                "decv returned an unsupported audio input status: {status:?}"
                            )));
                        }
                    }
                }
                output => {
                    return Err(decv_decode_message(format!(
                        "decv returned an unsupported audio decoder output: {output:?}"
                    )));
                }
            }
        }
        Ok(())
    }

    fn play(&self) -> MediaResult<()> {
        self.output.play()
    }

    fn pause(&self) -> MediaResult<()> {
        self.output.pause()
    }

    fn reload(&mut self, autoplay: bool) -> MediaResult<()> {
        self.output.pause()?;
        self.cursor.rewind();
        self.decoder.flush();
        self.pending_packet = None;
        self.draining = false;
        self.decode_complete = false;
        self.minimum_pts = Some(Duration::ZERO);
        self.output.reset(Duration::ZERO);
        self.device_clock_released = false;
        if autoplay {
            self.output.play()?;
        }
        Ok(())
    }

    fn seek(&mut self, target: MediaTime, position: Duration) -> MediaResult<()> {
        self.output.pause()?;
        self.cursor
            .seek_to_time(target)
            .map_err(|error| decv_container_error("failed to seek MP4 audio cursor", error))?
            .ok_or_else(|| {
                MediaError::new(
                    MediaErrorKind::UnsupportedOperation,
                    format!("no MP4 audio sample exists at or before {position:?}"),
                    MediaRecovery::None,
                )
            })?;
        self.decoder.flush();
        self.pending_packet = None;
        self.draining = false;
        self.decode_complete = false;
        self.minimum_pts = Some(position);
        self.output.reset(position);
        self.device_clock_released = false;
        Ok(())
    }

    fn set_volume(&mut self, volume: f64) {
        self.output.set_volume(volume);
    }

    fn set_muted(&mut self, muted: bool) {
        self.output.set_muted(muted);
    }

    fn position(&self) -> Duration {
        self.output.position()
    }

    fn clock(&self) -> AudioClock {
        self.output.clock()
    }

    fn is_decode_complete(&self) -> bool {
        self.decode_complete
    }

    fn is_drained(&self) -> bool {
        self.output.is_drained()
    }

    fn uses_device_clock(&self) -> bool {
        !self.device_clock_released
    }

    fn release_device_clock_if_complete(&mut self) -> Option<Duration> {
        if self.device_clock_released || !self.decode_complete || !self.is_drained() {
            return None;
        }
        self.device_clock_released = true;
        Some(self.position())
    }
}

fn convert_frame(
    frame: DecodedVideoFrame,
    handle: SurfaceHandle,
    sequence: u64,
) -> MediaResult<VideoFrame> {
    frame
        .validate()
        .map_err(|error| decv_decode_error("decv produced an invalid frame", error))?;
    let cpu = match frame.storage {
        FrameStorage::Cpu(cpu) => cpu,
        _ => {
            return Err(MediaError::new(
                MediaErrorKind::VideoOutput,
                "decv produced a non-CPU frame that this adapter does not support",
                MediaRecovery::None,
            ));
        }
    };

    let coded_size = device_size(frame.format.coded_size)?;
    let visible_rect = Bounds {
        origin: point(
            device_pixel(frame.format.visible_rect.x)?,
            device_pixel(frame.format.visible_rect.y)?,
        ),
        size: size(
            device_pixel(frame.format.visible_rect.width)?,
            device_pixel(frame.format.visible_rect.height)?,
        ),
    };
    let display_size = device_size(frame.format.display_size)?;
    let [y, uv] = match frame.format.pixel_format {
        PixelFormat::Nv12 => {
            let [y, uv] = cpu.planes.as_slice() else {
                return Err(decv_decode_message(
                    "decv NV12 frame does not contain exactly two planes",
                ));
            };
            [surface_plane(y, "Y")?, surface_plane(uv, "UV")?]
        }
        format
        @ (PixelFormat::I420 | PixelFormat::I422 | PixelFormat::I440 | PixelFormat::I444) => {
            let [y, u, v] = cpu.planes.as_slice() else {
                return Err(decv_decode_message(
                    "decv planar YUV frame does not contain exactly three planes",
                ));
            };
            let (subsampling_x, subsampling_y) = format
                .chroma_subsampling()
                .expect("planar YUV pixel format has chroma subsampling");
            [
                surface_plane(y, "Y")?,
                planar_chroma_to_nv12(
                    u,
                    v,
                    frame.format.coded_size,
                    subsampling_x,
                    subsampling_y,
                    8,
                )?,
            ]
        }
        PixelFormat::PlanarYuv16 {
            bit_depth,
            subsampling_x,
            subsampling_y,
        } => {
            let [y, u, v] = cpu.planes.as_slice() else {
                return Err(decv_decode_message(
                    "decv high-bit-depth planar YUV frame does not contain exactly three planes",
                ));
            };
            [
                planar_luma_to_8bit(y, frame.format.coded_size, bit_depth)?,
                planar_chroma_to_nv12(
                    u,
                    v,
                    frame.format.coded_size,
                    subsampling_x,
                    subsampling_y,
                    bit_depth,
                )?,
            ]
        }
        format => {
            return Err(MediaError::new(
                MediaErrorKind::UnsupportedCodec,
                format!("decv produced unsupported pixel format {format:?}"),
                MediaRecovery::None,
            ));
        }
    };
    let surface = SurfaceFrame::new(
        handle,
        sequence,
        coded_size,
        visible_rect,
        display_size,
        SurfaceFormat::Nv12,
        [y, uv],
        surface_color_info(frame.format.color, frame.format.coded_size.height),
    )
    .map_err(|error| decv_video_output_error("GPUI rejected the decv NV12 frame", error))?;

    Ok(VideoFrame::new(
        Arc::new(surface),
        media_time_to_duration(frame.pts),
        media_time_to_duration(frame.duration),
    ))
}

fn surface_plane(plane: &decv::CpuPlane, name: &str) -> MediaResult<SurfacePlane> {
    let stride = u32::try_from(plane.stride).map_err(|error| {
        decv_video_output_error(format!("decv {name} stride exceeds u32"), error)
    })?;
    Ok(match &plane.bytes {
        CpuBuffer::Bytes(bytes) => SurfacePlane::with_offset(bytes.clone(), plane.offset, stride),
        CpuBuffer::Aligned(bytes) => {
            SurfacePlane::with_shared_offset(bytes.clone(), plane.offset, stride)
        }
        CpuBuffer::Words(_) => {
            return Err(decv_video_output_message(format!(
                "decv {name} plane uses 16-bit samples where an 8-bit plane was required"
            )));
        }
    })
}

fn planar_luma_to_8bit(
    plane: &decv::CpuPlane,
    coded_size: DecvSize,
    bit_depth: u8,
) -> MediaResult<SurfacePlane> {
    let width = usize::try_from(coded_size.width)
        .map_err(|error| decv_video_output_error("decv frame width exceeds usize", error))?;
    let height = usize::try_from(coded_size.height)
        .map_err(|error| decv_video_output_error("decv frame height exceeds usize", error))?;
    let len = width
        .checked_mul(height)
        .ok_or_else(|| decv_video_output_message("decv luma plane size overflowed"))?;
    let mut output = Vec::with_capacity(len);
    let shift = bit_depth.saturating_sub(8);
    for y in 0..height {
        for x in 0..width {
            let sample = planar_sample(plane, x, y, bit_depth)?;
            output.push((sample >> shift).min(u32::from(u8::MAX)) as u8);
        }
    }
    Ok(SurfacePlane::new(output, coded_size.width))
}

fn planar_chroma_to_nv12(
    u: &decv::CpuPlane,
    v: &decv::CpuPlane,
    coded_size: DecvSize,
    subsampling_x: u8,
    subsampling_y: u8,
    bit_depth: u8,
) -> MediaResult<SurfacePlane> {
    let width = usize::try_from(coded_size.width)
        .map_err(|error| decv_video_output_error("decv frame width exceeds usize", error))?;
    let height = usize::try_from(coded_size.height)
        .map_err(|error| decv_video_output_error("decv frame height exceeds usize", error))?;
    let output_columns = width.div_ceil(2);
    let output_rows = height.div_ceil(2);
    let output_stride = output_columns
        .checked_mul(2)
        .ok_or_else(|| decv_video_output_message("decv chroma stride overflowed"))?;
    let output_len = output_stride
        .checked_mul(output_rows)
        .ok_or_else(|| decv_video_output_message("decv chroma plane size overflowed"))?;
    let source_width = width.div_ceil(1usize << subsampling_x);
    let source_height = height.div_ceil(1usize << subsampling_y);
    let horizontal_samples = if subsampling_x == 0 { 2 } else { 1 };
    let vertical_samples = if subsampling_y == 0 { 2 } else { 1 };
    let shift = bit_depth.saturating_sub(8);
    let mut output = Vec::with_capacity(output_len);

    for output_y in 0..output_rows {
        let source_y = if subsampling_y == 0 {
            output_y * 2
        } else {
            output_y
        };
        for output_x in 0..output_columns {
            let source_x = if subsampling_x == 0 {
                output_x * 2
            } else {
                output_x
            };
            for plane in [u, v] {
                let mut sum = 0u32;
                let mut count = 0u32;
                for y_offset in 0..vertical_samples {
                    let y = (source_y + y_offset).min(source_height.saturating_sub(1));
                    for x_offset in 0..horizontal_samples {
                        let x = (source_x + x_offset).min(source_width.saturating_sub(1));
                        sum = sum.saturating_add(planar_sample(plane, x, y, bit_depth)?);
                        count += 1;
                    }
                }
                let sample = ((sum + count / 2) / count) >> shift;
                output.push(sample.min(u32::from(u8::MAX)) as u8);
            }
        }
    }

    let output_stride = u32::try_from(output_stride)
        .map_err(|error| decv_video_output_error("decv chroma stride exceeds u32", error))?;
    Ok(SurfacePlane::new(output, output_stride))
}

fn planar_sample(plane: &decv::CpuPlane, x: usize, y: usize, bit_depth: u8) -> MediaResult<u32> {
    let bytes_per_sample = if bit_depth > 8 { 2 } else { 1 };
    let row = y
        .checked_mul(plane.stride)
        .and_then(|offset| plane.offset.checked_add(offset))
        .ok_or_else(|| decv_video_output_message("decv planar sample offset overflowed"))?;
    let index = x
        .checked_mul(bytes_per_sample)
        .and_then(|offset| row.checked_add(offset))
        .ok_or_else(|| decv_video_output_message("decv planar sample index overflowed"))?;
    if bytes_per_sample == 1 {
        plane
            .bytes
            .get(index)
            .copied()
            .map(u32::from)
            .ok_or_else(|| decv_video_output_message("decv planar sample exceeds its plane"))
    } else {
        let bytes = plane
            .bytes
            .get(index..index.saturating_add(2))
            .ok_or_else(|| decv_video_output_message("decv planar sample exceeds its plane"))?;
        Ok(u32::from(u16::from_ne_bytes([bytes[0], bytes[1]])))
    }
}

fn surface_color_info(color: ColorInfo, coded_height: u32) -> SurfaceColorInfo {
    let matrix = match color.matrix {
        DecvColorMatrix::Bt601 | DecvColorMatrix::Bt470Bg | DecvColorMatrix::Smpte170M => {
            YuvMatrix::Bt601
        }
        DecvColorMatrix::Bt709 => YuvMatrix::Bt709,
        DecvColorMatrix::Unspecified
        | DecvColorMatrix::Identity
        | DecvColorMatrix::Bt2020NonConstantLuminance
        | DecvColorMatrix::Bt2020ConstantLuminance
        | DecvColorMatrix::Other(_) => {
            if coded_height >= 720 {
                YuvMatrix::Bt709
            } else {
                YuvMatrix::Bt601
            }
        }
        _ => YuvMatrix::Bt709,
    };
    let range = match color.range {
        DecvColorRange::Full => ColorRange::Full,
        DecvColorRange::Limited | DecvColorRange::Unspecified => ColorRange::Limited,
        _ => ColorRange::Limited,
    };
    SurfaceColorInfo { matrix, range }
}

fn open_media_input(source: &MediaSource) -> MediaResult<DecvMediaInput> {
    let uri = url::Url::parse(source.uri()).map_err(|error| {
        MediaError::from_error(
            MediaErrorKind::InvalidInput,
            MediaRecovery::None,
            "invalid media URI",
            error,
        )
    })?;
    match uri.scheme() {
        "file" => {
            let path = uri.to_file_path().map_err(|_| {
                MediaError::invalid_input("cannot convert media URI to a local path")
            })?;
            File::open(&path)
                .map(|file| Arc::new(file) as DecvMediaInput)
                .map_err(|error| {
                    MediaError::io(format!("failed to open {}", path.display()), error)
                })
        }
        "http" | "https" => open_http_input(source),
        scheme => Err(MediaError::new(
            MediaErrorKind::UnsupportedOperation,
            format!("decv does not support media URI scheme {scheme:?}"),
            MediaRecovery::None,
        )),
    }
}

fn open_http_input(source: &MediaSource) -> MediaResult<DecvMediaInput> {
    let options = source.network_options();
    let mut builder = HttpRangeInput::builder(source.uri()).cache_config(http_range_cache_config());
    for (name, value) in options.headers() {
        builder = builder
            .header(name, value)
            .map_err(|error| decv_network_configuration_error(source, error))?;
    }
    if let Some(user_agent) = options.user_agent()
        && !options
            .headers()
            .keys()
            .any(|name| name.eq_ignore_ascii_case("user-agent"))
    {
        builder = builder
            .header("User-Agent", user_agent)
            .map_err(|error| decv_network_configuration_error(source, error))?;
    }
    if !options
        .headers()
        .keys()
        .any(|name| name.eq_ignore_ascii_case("authorization"))
        && (options.user_id().is_some() || options.user_password().is_some())
    {
        let credentials = format!(
            "{}:{}",
            options.user_id().unwrap_or_default(),
            options.user_password().unwrap_or_default()
        );
        builder = builder
            .header(
                "Authorization",
                format!("Basic {}", BASE64_STANDARD.encode(credentials)),
            )
            .map_err(|error| decv_network_configuration_error(source, error))?;
    }
    builder
        .build()
        .map(|input| Arc::new(input) as DecvMediaInput)
        .map_err(|error| {
            MediaError::from_error(
                MediaErrorKind::Network { status: None },
                MediaRecovery::Retry,
                format!("failed to open HTTP Range media {}", source.display_name()),
                source.redact_error_message(&error.to_string()),
            )
        })
}

fn http_range_cache_config() -> RangeCacheConfig {
    RangeCacheConfig::new(
        NonZeroUsize::new(HTTP_CACHE_BLOCK_BYTES).unwrap(),
        NonZeroUsize::new(HTTP_CACHE_MAXIMUM_BLOCKS).unwrap(),
    )
}

fn decv_network_configuration_error(source: &MediaSource, error: std::io::Error) -> MediaError {
    MediaError::from_error(
        MediaErrorKind::InvalidInput,
        MediaRecovery::None,
        format!(
            "invalid decv HTTP configuration for {}",
            source.display_name()
        ),
        source.redact_error_message(&error.to_string()),
    )
}

fn video_track_index(demuxer: &DecvDemuxer) -> MediaResult<usize> {
    demuxer
        .movie()
        .tracks()
        .iter()
        .enumerate()
        .find(|(_, track)| track.kind() == TrackKind::Video && !track.samples().is_empty())
        .map(|(index, _)| index)
        .ok_or_else(|| decv_container_message("MP4 contains no non-empty video track"))
}

fn audio_track_index(demuxer: &DecvDemuxer) -> Option<usize> {
    demuxer
        .movie()
        .tracks()
        .iter()
        .enumerate()
        .find(|(_, track)| {
            track.kind() == TrackKind::Audio
                && !track.samples().is_empty()
                && track.audio_decoder_config_for_sample(0).is_ok()
        })
        .map(|(index, _)| index)
}

fn video_sample_time(
    cursor: &PacketCursor<'_, DecvMediaInput>,
    sample_index: usize,
) -> MediaResult<MediaTime> {
    let sample = cursor
        .track()
        .samples()
        .get(sample_index)
        .ok_or_else(|| decv_container_message("selected MP4 video sample is missing"))?;
    let offset = cursor.track().presentation_time_offset().map_err(|error| {
        decv_container_error("failed to map the selected MP4 video timestamp", error)
    })?;
    let value = sample
        .presentation_time()
        .checked_add(offset.value)
        .ok_or_else(|| decv_container_message("selected MP4 video timestamp overflowed"))?;
    Ok(MediaTime::new(value, cursor.track().media_timescale()))
}

fn track_presentation_duration(track: &Track) -> MediaResult<Option<Duration>> {
    let offset = track.presentation_time_offset().map_err(|error| {
        decv_container_error(
            "failed to map MP4 track duration to the movie timeline",
            error,
        )
    })?;
    let mut end = None;
    for sample in track.samples() {
        let sample_end = sample
            .presentation_time()
            .checked_add(i64::from(sample.duration()))
            .and_then(|value| value.checked_add(offset.value))
            .ok_or_else(|| decv_container_message("MP4 track end timestamp overflowed"))?;
        end = Some(end.map_or(sample_end, |end: i64| end.max(sample_end)));
    }
    end.filter(|end| *end >= 0)
        .map(|end| duration_from_ratio(end as u64, track.media_timescale().get()))
        .transpose()
}

fn duration_from_ratio(value: u64, timescale: u32) -> MediaResult<Duration> {
    if timescale == 0 {
        return Err(decv_container_message("media timescale is zero"));
    }
    let seconds = value / u64::from(timescale);
    let remainder = value % u64::from(timescale);
    let nanos = (u128::from(remainder) * 1_000_000_000u128) / u128::from(timescale);
    Ok(Duration::new(
        seconds,
        u32::try_from(nanos)
            .map_err(|error| decv_container_error("media timestamp is invalid", error))?,
    ))
}

fn media_time_to_duration(time: Option<MediaTime>) -> Option<Duration> {
    let time = time?;
    if time.value < 0 {
        return None;
    }
    duration_from_ratio(time.value as u64, time.timescale.get()).ok()
}

fn duration_to_media_time(duration: Duration) -> MediaResult<MediaTime> {
    let value = i64::try_from(duration.as_nanos()).map_err(|error| {
        MediaError::from_error(
            MediaErrorKind::InvalidInput,
            MediaRecovery::None,
            "seek target cannot be represented by decv MediaTime",
            error,
        )
    })?;
    MediaTime::from_parts(value, MEDIA_TIME_SCALE)
        .ok_or_else(|| MediaError::invalid_input("invalid decv media timescale"))
}

fn audio_clock_wait(
    video_timestamp: Duration,
    audio_position: Duration,
    preroll: bool,
    uses_device_clock: bool,
) -> Option<Duration> {
    if preroll || !uses_device_clock {
        return None;
    }
    Some(video_timestamp.saturating_sub(audio_position))
}

fn device_size(value: DecvSize) -> MediaResult<gpui::Size<DevicePixels>> {
    Ok(size(
        device_pixel(value.width)?,
        device_pixel(value.height)?,
    ))
}

fn device_pixel(value: u32) -> MediaResult<DevicePixels> {
    Ok(DevicePixels(i32::try_from(value).map_err(|error| {
        decv_video_output_error("video dimension exceeds i32", error)
    })?))
}

#[derive(Clone, Copy)]
struct PresentationClock {
    media_anchor: Duration,
    wall_anchor: Instant,
}

impl PresentationClock {
    fn new(media_anchor: Duration) -> Self {
        Self {
            media_anchor,
            wall_anchor: Instant::now(),
        }
    }

    fn deadline(self, timestamp: Duration) -> Instant {
        self.wall_anchor
            .checked_add(timestamp.saturating_sub(self.media_anchor))
            .unwrap_or(self.wall_anchor)
    }
}

struct TimelineState {
    position: Duration,
    duration: Option<Duration>,
    playing: bool,
    wall_anchor: Instant,
    audio_clock: Option<AudioClock>,
    seeking: bool,
}

impl TimelineState {
    fn new(duration: Option<Duration>) -> Self {
        Self {
            position: Duration::ZERO,
            duration,
            playing: false,
            wall_anchor: Instant::now(),
            audio_clock: None,
            seeking: false,
        }
    }

    fn set_duration(&mut self, duration: Option<Duration>) {
        self.duration = duration;
        if let Some(duration) = duration {
            self.position = self.position.min(duration);
        }
    }

    fn current_position(&self, now: Instant) -> Duration {
        let position = if self.seeking {
            self.position
        } else if self.playing {
            self.audio_clock.as_ref().map_or_else(
                || {
                    self.position
                        .saturating_add(now.saturating_duration_since(self.wall_anchor))
                },
                AudioClock::position,
            )
        } else {
            self.position
        };
        self.duration
            .map_or(position, |duration| position.min(duration))
    }

    fn snapshot(&self, now: Instant) -> PlaybackTimeline {
        PlaybackTimeline::new(self.current_position(now), self.duration, true)
    }

    fn set_playing(&mut self, playing: bool, now: Instant) {
        self.position = self.current_position(now);
        self.playing = playing;
        self.wall_anchor = now;
    }

    fn begin_seek(&mut self, position: Duration, playing: bool, now: Instant) {
        self.position = self
            .duration
            .map_or(position, |duration| position.min(duration));
        self.playing = playing;
        self.wall_anchor = now;
        self.audio_clock = None;
        self.seeking = true;
    }

    fn complete_seek(
        &mut self,
        position: Duration,
        playing: bool,
        audio_clock: Option<AudioClock>,
        now: Instant,
    ) {
        self.position = self
            .duration
            .map_or(position, |duration| position.min(duration));
        self.playing = playing;
        self.wall_anchor = now;
        self.audio_clock = audio_clock;
        self.seeking = false;
    }

    fn present(&mut self, position: Duration, now: Instant) {
        self.position = self
            .duration
            .map_or(position, |duration| position.min(duration));
        self.wall_anchor = now;
    }

    fn finish(&mut self, now: Instant) {
        self.position = self.duration.unwrap_or_else(|| self.current_position(now));
        self.playing = false;
        self.wall_anchor = now;
        self.seeking = false;
    }

    fn release_audio_clock(&mut self, position: Duration, now: Instant) {
        self.position = self
            .duration
            .map_or(position, |duration| position.min(duration));
        self.wall_anchor = now;
        self.audio_clock = None;
        self.seeking = false;
    }
}

trait MutexExt<T> {
    fn lock_unpoisoned(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_unpoisoned(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|error| error.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Command, DEFAULT_INTERACTIVE_SEEK_MAXIMUM_INPUT_SAMPLES, DecvBackend, DecvBackendOptions,
        DecvParallelism, SeekPlaybackState, TimelineState, audio_clock_wait, coalesce_seek_command,
        http_range_cache_config, network_prefetch_is_due, open_media_input,
        seek_generation_is_current,
    };
    use std::{
        io::{Read, Write},
        net::TcpListener,
        num::NonZeroUsize,
        sync::{Arc, mpsc},
        thread,
        time::{Duration, Instant},
    };

    use crate::{
        FrameExtractorBackendRequest, MediaBackend, MediaErrorKind, MediaSource,
        NetworkSourceOptions, SeekMode,
    };

    #[test]
    fn backend_parallelism_defaults_to_auto_and_is_configurable() {
        assert_eq!(
            DecvBackend::default().options().selected_parallelism(),
            DecvParallelism::Auto
        );
        assert_eq!(
            DecvBackend::default()
                .options()
                .selected_interactive_seek_maximum_input_samples(),
            DEFAULT_INTERACTIVE_SEEK_MAXIMUM_INPUT_SAMPLES
        );
        assert_eq!(
            DecvBackend::new()
                .parallelism(DecvParallelism::Serial)
                .options()
                .selected_parallelism(),
            DecvParallelism::Serial
        );

        let threads = NonZeroUsize::new(3).unwrap();
        let options = DecvBackendOptions::new().parallelism(DecvParallelism::Threads(threads));
        assert_eq!(
            DecvBackend::with_options(options)
                .options()
                .selected_parallelism(),
            DecvParallelism::Threads(threads)
        );
        assert_eq!(
            DecvBackend::new()
                .interactive_seek_maximum_input_samples(8)
                .options()
                .selected_interactive_seek_maximum_input_samples(),
            8
        );
    }

    #[test]
    fn backend_opens_a_lazy_network_frame_extractor() {
        let source = MediaSource::from_uri("http://127.0.0.1:9/video.mp4").unwrap();
        let extractor = DecvBackend::new().open_frame_extractor(FrameExtractorBackendRequest {
            source,
            timeout: Duration::from_secs(1),
        });

        assert!(extractor.is_ok());
    }

    #[test]
    fn http_cache_uses_playback_sized_blocks_and_capacity() {
        let config = http_range_cache_config();
        assert_eq!(config.block_size().get(), 1024 * 1024);
        assert_eq!(config.maximum_cached_bytes(), 64 * 1024 * 1024);
    }

    #[test]
    fn network_prefetch_rearms_at_low_water_or_after_seek() {
        let initial = 10 * 1024 * 1024;
        let last_request = Some((4, initial));

        assert!(!network_prefetch_is_due(
            last_request,
            4,
            initial + 3 * 1024 * 1024
        ));
        assert!(network_prefetch_is_due(
            last_request,
            4,
            initial + 4 * 1024 * 1024
        ));
        assert!(network_prefetch_is_due(last_request, 5, initial));
        assert!(network_prefetch_is_due(last_request, 4, initial - 1));
    }

    #[test]
    fn video_waits_for_the_device_audio_clock_except_during_preroll() {
        let video = Duration::from_secs(2);
        let audio = Duration::from_millis(750);
        assert_eq!(
            audio_clock_wait(video, audio, false, true),
            Some(Duration::from_millis(1_250))
        );
        assert_eq!(audio_clock_wait(video, audio, true, true), None);
        assert_eq!(audio_clock_wait(video, audio, false, false), None);
        assert_eq!(
            audio_clock_wait(audio, video, false, true),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn timeline_stays_pinned_until_video_preroll_completes() {
        let start = Instant::now();
        let mut timeline = TimelineState::new(Some(Duration::from_secs(20)));
        timeline.set_playing(true, start);
        timeline.begin_seek(
            Duration::from_secs(10),
            true,
            start + Duration::from_secs(1),
        );

        assert_eq!(
            timeline.snapshot(start + Duration::from_secs(5)).position(),
            Duration::from_secs(10)
        );

        timeline.complete_seek(
            Duration::from_secs(10),
            true,
            None,
            start + Duration::from_secs(5),
        );
        assert_eq!(
            timeline.snapshot(start + Duration::from_secs(7)).position(),
            Duration::from_secs(12)
        );
    }

    #[test]
    fn consecutive_seek_commands_collapse_to_the_latest_generation() {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(Command::Seek {
                position: Duration::from_secs(2),
                mode: SeekMode::Accurate,
                generation: 2,
            })
            .unwrap();
        sender.send(Command::Pause).unwrap();
        sender
            .send(Command::Seek {
                position: Duration::from_secs(3),
                mode: SeekMode::Accurate,
                generation: 3,
            })
            .unwrap();

        let mut deferred = None;
        let command = coalesce_seek_command(
            Command::Seek {
                position: Duration::from_secs(1),
                mode: SeekMode::Accurate,
                generation: 1,
            },
            &receiver,
            &mut deferred,
        );
        assert!(matches!(command, Command::Seek { generation: 2, .. }));
        assert!(matches!(deferred, Some(Command::Pause)));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            Command::Seek { generation: 3, .. }
        ));
    }

    #[test]
    fn stale_seek_generation_cannot_complete_preroll() {
        assert!(seek_generation_is_current(4, 4));
        assert!(!seek_generation_is_current(3, 4));
    }

    #[test]
    fn completed_exact_interactive_seek_can_only_reuse_the_same_target() {
        let target = Duration::from_secs(7);
        let mut state = SeekPlaybackState::new(3);
        state.begin_interactive_preroll(Some(target));
        assert!(state.interactive_preroll);
        assert!(!state.can_reuse_interactive_exact(target));

        state.complete_interactive_preroll();
        assert!(!state.interactive_preroll);
        assert!(state.can_reuse_interactive_exact(target));
        assert!(!state.can_reuse_interactive_exact(target + Duration::from_millis(1)));

        state.reset_interactive();
        assert!(!state.can_reuse_interactive_exact(target));
    }

    #[test]
    fn http_input_forwards_credentials_and_reads_ranges() {
        let listener = match TcpListener::bind(("127.0.0.1", 0)) {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("failed to bind HTTP Range test server: {error}"),
        };
        let address = listener.local_addr().unwrap();
        let server_contents: Arc<[u8]> = Arc::from(*b"0123456789abcdef");
        let (requests, received_requests) = mpsc::channel();
        let server = thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let mut stream = stream.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let count = stream.read(&mut chunk).unwrap();
                    assert_ne!(count, 0, "client closed before completing HTTP headers");
                    request.extend_from_slice(&chunk[..count]);
                }
                let request = String::from_utf8(request).unwrap();
                let range = request
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Range: bytes=")
                            .or_else(|| line.strip_prefix("range: bytes="))
                    })
                    .unwrap();
                let (start, end) = range.split_once('-').unwrap();
                let start = start.parse::<usize>().unwrap();
                let end = end.parse::<usize>().unwrap();
                let body = &server_contents[start..=end];
                requests.send(request).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 206 Partial Content\r\n\
                     Content-Range: bytes {start}-{end}/{}\r\n\
                     Content-Length: {}\r\n\
                     ETag: \"gpui-media-test\"\r\n\
                     Connection: close\r\n\r\n",
                    server_contents.len(),
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            }
        });

        let source = MediaSource::from_uri(format!("http://{address}/private/video.mp4"))
            .unwrap()
            .with_network_options(
                NetworkSourceOptions::default()
                    .with_bearer_token("range-secret")
                    .unwrap()
                    .with_user_agent("gpui-media-network-test"),
            );
        let input = open_media_input(&source).unwrap();
        let mut buffer = [0; 5];
        assert_eq!(input.read_at(6, &mut buffer).unwrap(), buffer.len());
        assert_eq!(&buffer, b"6789a");
        drop(input);

        let requests = [
            received_requests.recv().unwrap(),
            received_requests.recv().unwrap(),
        ];
        server.join().unwrap();
        for request in requests {
            let request = request.to_ascii_lowercase();
            assert!(request.contains("authorization: bearer range-secret"));
            assert!(request.contains("user-agent: gpui-media-network-test"));
        }
    }

    #[test]
    fn http_input_rejects_headers_managed_by_decv() {
        let source = MediaSource::from_uri("http://127.0.0.1:9/video.mp4")
            .unwrap()
            .with_network_options(
                NetworkSourceOptions::default()
                    .with_header("Range", "bytes=0-1")
                    .unwrap(),
            );

        let error = match open_media_input(&source) {
            Ok(_) => panic!("decv accepted an HTTP header it must manage internally"),
            Err(error) => error,
        };
        assert_eq!(error.kind, MediaErrorKind::InvalidInput);
        assert!(error.message.contains("managed by decv-network"));
    }
}
