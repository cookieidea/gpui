use std::{
    fs::File,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

mod audio_output;

use audio_output::{AudioClock, AudioOutput};
use decv::{
    AudioDecodeInputStatus, AudioDecodeOutput, AudioDecoder, AudioFormat, AudioPacketCursor,
    ColorInfo, ColorMatrix as DecvColorMatrix, ColorRange as DecvColorRange, DecodeInputStatus,
    DecodeOutput, DecodedVideoFrame, EncodedAudioPacket, EncodedVideoPacket, FrameStorage,
    H264Decoder, H264Mp4SeekController, H264Parallelism, MediaTime, Mp4Demuxer, PacketCursor,
    PixelFormat, Size as DecvSize, SoftwareAudioDecoder, Track, TrackKind, VideoDecoder,
};
use gpui::{
    Bounds, ColorRange, DevicePixels, SurfaceColorInfo, SurfaceFormat, SurfaceFrame, SurfaceHandle,
    SurfacePlane, YuvMatrix, point, size,
};

use crate::{
    MediaBackend, MediaBackendEvent, MediaCapabilities, MediaError, MediaErrorKind,
    MediaOutputSink, MediaPlaybackRequest, MediaPlaybackSession, MediaRecovery, MediaResult,
    MediaSource, PlaybackTimeline, SeekMode, VideoFrame,
};

const MEDIA_TIME_SCALE: u32 = 1_000_000_000;
const AUDIO_BUFFER_TARGET: Duration = Duration::from_secs(2);
const AUDIO_PREROLL_TARGET: Duration = Duration::from_millis(100);
const AUDIO_CLOCK_POLL_INTERVAL: Duration = Duration::from_millis(5);
const VIDEO_SEEK_CHECKPOINT_LIMIT: usize = 4;
const VIDEO_SEEK_CHECKPOINT_REFERENCE_BYTES: usize = 128 * 1024 * 1024;

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecvBackendOptions {
    parallelism: DecvParallelism,
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
}

/// Pure-Rust MP4 H.264/AAC playback backed by `decv`.
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
}

struct DecvSession {
    commands: mpsc::Sender<Command>,
    timeline: Arc<Mutex<TimelineState>>,
    latest_seek_generation: Arc<AtomicU64>,
    has_audio: bool,
    worker: Option<JoinHandle<()>>,
}

impl DecvSession {
    fn open(
        source: MediaSource,
        output: MediaOutputSink,
        options: DecvBackendOptions,
    ) -> MediaResult<Self> {
        let path = local_path(&source)?;
        let probe = probe_mp4(&path)?;
        let timeline = Arc::new(Mutex::new(TimelineState::new(probe.duration)));
        let video_track_index = probe.video_track_index;
        let audio_track_index = probe.audio_track_index;
        let (commands, receiver) = mpsc::channel();
        let worker_timeline = timeline.clone();
        let latest_seek_generation = Arc::new(AtomicU64::new(0));
        let worker_latest_seek_generation = latest_seek_generation.clone();
        let worker_path = path;
        let worker = thread::Builder::new()
            .name("gpui-video-decv".to_owned())
            .spawn(move || {
                if let Err(error) = playback_worker(
                    &worker_path,
                    video_track_index,
                    audio_track_index,
                    receiver,
                    &worker_timeline,
                    &worker_latest_seek_generation,
                    &output,
                    options.parallelism,
                ) {
                    log::error!(
                        "decv playback failed for {}: {error}",
                        worker_path.display()
                    );
                    output.emit(MediaBackendEvent::Error(Arc::new(error)));
                }
            })
            .map_err(|error| MediaError::io("failed to start decv playback thread", error))?;

        Ok(Self {
            commands,
            timeline,
            latest_seek_generation,
            has_audio: audio_track_index.is_some(),
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
            audio: self.has_audio,
            seeking: true,
            accurate_seeking: true,
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
        let mut timeline = self.timeline.lock_unpoisoned();
        let target = timeline
            .duration
            .map_or(position, |duration| position.min(duration));
        let playing = timeline.playing;
        timeline.begin_seek(target, playing, Instant::now());
        drop(timeline);
        let generation = self
            .latest_seek_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
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

struct Probe {
    video_track_index: usize,
    audio_track_index: Option<usize>,
    duration: Option<Duration>,
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

fn probe_mp4(path: &Path) -> MediaResult<Probe> {
    let demuxer =
        Mp4Demuxer::open(File::open(path).map_err(|error| {
            MediaError::io(format!("failed to open {}", path.display()), error)
        })?)
        .map_err(|error| {
            decv_container_error(format!("failed to parse MP4 {}", path.display()), error)
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
    Ok(Probe {
        video_track_index,
        audio_track_index,
        duration,
    })
}

fn playback_worker(
    path: &Path,
    video_track_index: usize,
    audio_track_index: Option<usize>,
    commands: mpsc::Receiver<Command>,
    shared_timeline: &Mutex<TimelineState>,
    latest_seek_generation: &AtomicU64,
    output: &MediaOutputSink,
    parallelism: DecvParallelism,
) -> MediaResult<()> {
    let demuxer =
        Mp4Demuxer::open(File::open(path).map_err(|error| {
            MediaError::io(format!("failed to open {}", path.display()), error)
        })?)
        .map_err(|error| {
            decv_container_error(format!("failed to parse MP4 {}", path.display()), error)
        })?;
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

    let mut decoder = H264Decoder::new();
    decoder
        .set_parallelism(parallelism.into())
        .map_err(|error| decv_decode_error("failed to configure decv H.264 parallelism", error))?;
    decoder.configure(decoder_config).map_err(|error| {
        MediaError::new(
            MediaErrorKind::UnsupportedCodec,
            format!("failed to configure decv H.264 decoder: {error:?}"),
            MediaRecovery::None,
        )
    })?;
    let mut seek_controller = H264Mp4SeekController::new(
        video_track_index,
        VIDEO_SEEK_CHECKPOINT_LIMIT,
        VIDEO_SEEK_CHECKPOINT_REFERENCE_BYTES,
    );
    let mut audio = audio_track_index
        .map(|track_index| DecvAudioPlayback::open(&demuxer, track_index))
        .transpose()?;
    shared_timeline.lock_unpoisoned().audio_clock = audio.as_ref().map(DecvAudioPlayback::clock);

    let surface_handle = SurfaceHandle::new();
    let mut sequence = 0u64;
    let mut pending_packet = None;
    let mut pending_frame = None;
    let mut draining = false;
    let mut discontinuity = false;
    let mut playing = false;
    let mut preroll = false;
    let mut ended = false;
    let mut minimum_pts = None;
    let mut handled_seek_generation = latest_seek_generation.load(Ordering::Acquire);
    let mut deferred_command = None;
    let mut clock = PresentationClock::new(Duration::ZERO);

    loop {
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
                &mut handled_seek_generation,
                &mut pending_packet,
                &mut pending_frame,
                &mut draining,
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
                &mut handled_seek_generation,
                &mut pending_packet,
                &mut pending_frame,
                &mut draining,
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

        if pending_frame.is_none() {
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
                VideoDecodeStep::Frame(frame) => pending_frame = Some(frame),
                VideoDecodeStep::EndOfStream => {
                    let audio_pending = audio
                        .as_ref()
                        .is_some_and(|audio| !audio.is_decode_complete() || !audio.is_drained());
                    if audio_pending {
                        match commands.recv_timeout(AUDIO_CLOCK_POLL_INTERVAL) {
                            Ok(command) => {
                                let command = coalesce_seek_command(
                                    command,
                                    &commands,
                                    &mut deferred_command,
                                );
                                if apply_command(
                                    command,
                                    &mut cursor,
                                    &mut decoder,
                                    &mut seek_controller,
                                    &mut handled_seek_generation,
                                    &mut pending_packet,
                                    &mut pending_frame,
                                    &mut draining,
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
            }
            let command = deferred_command.take().or_else(|| commands.try_recv().ok());
            if let Some(command) = command {
                let command = coalesce_seek_command(command, &commands, &mut deferred_command);
                if apply_command(
                    command,
                    &mut cursor,
                    &mut decoder,
                    &mut seek_controller,
                    &mut handled_seek_generation,
                    &mut pending_packet,
                    &mut pending_frame,
                    &mut draining,
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

        let frame = pending_frame
            .as_ref()
            .expect("the decoder produced a pending frame");
        let timestamp = media_time_to_duration(frame.pts);
        if minimum_pts.is_some_and(|minimum| timestamp.is_some_and(|pts| pts < minimum)) {
            pending_frame = None;
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
        if !wait.is_zero() {
            match commands.recv_timeout(wait) {
                Ok(command) => {
                    let command = coalesce_seek_command(command, &commands, &mut deferred_command);
                    if apply_command(
                        command,
                        &mut cursor,
                        &mut decoder,
                        &mut seek_controller,
                        &mut handled_seek_generation,
                        &mut pending_packet,
                        &mut pending_frame,
                        &mut draining,
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

        let frame = pending_frame
            .take()
            .expect("the scheduled frame remains pending");
        let completed_preroll = preroll;
        if !seek_generation_is_current(
            handled_seek_generation,
            latest_seek_generation.load(Ordering::Acquire),
        ) {
            continue;
        }
        sequence = sequence.wrapping_add(1);
        let frame = Arc::new(convert_frame(frame, surface_handle.clone(), sequence)?);
        let timestamp = frame.timestamp().unwrap_or(timestamp);
        output.publish_video_frame(frame);
        preroll = false;
        if completed_preroll {
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

#[allow(clippy::too_many_arguments)]
fn apply_command(
    command: Command,
    cursor: &mut PacketCursor<'_, File>,
    decoder: &mut H264Decoder,
    seek_controller: &mut H264Mp4SeekController,
    handled_seek_generation: &mut u64,
    pending_packet: &mut Option<EncodedVideoPacket>,
    pending_frame: &mut Option<DecodedVideoFrame>,
    draining: &mut bool,
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
            if !*preroll && let Some(audio) = audio {
                audio.play()?;
            }
            *playing = true;
        }
        Command::Pause => {
            if let Some(audio) = audio {
                audio.pause()?;
            }
            *playing = false;
            *preroll = pending_frame.is_none() && cursor.next_sample_index() == 0 && !*ended;
        }
        Command::Reload {
            autoplay,
            generation,
        } => {
            cursor.rewind();
            reset_decode_timeline(decoder, pending_packet, pending_frame, draining);
            seek_controller.clear();
            *handled_seek_generation = generation;
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
            position,
            mode,
            generation,
        } => {
            let target = duration_to_media_time(position)?;
            let (seek_time, requires_discontinuity) = match mode {
                SeekMode::Accurate => {
                    let allow_forward_retarget = *preroll && !*draining && pending_packet.is_none();
                    let outcome = seek_controller
                        .begin_exact_seek(decoder, cursor, target, allow_forward_retarget)
                        .map_err(|error| {
                            decv_decode_error("failed to coordinate exact H.264 MP4 seek", error)
                        })?;
                    (target, outcome.requires_discontinuity())
                }
                SeekMode::KeyFrame => {
                    let sample_index = seek_controller
                        .begin_nearest_preview(decoder, cursor, target)
                        .map_err(|error| {
                            decv_decode_error("failed to coordinate H.264 MP4 seek preview", error)
                        })?;
                    (video_sample_time(cursor, sample_index)?, true)
                }
            };
            let seek_position = media_time_to_duration(Some(seek_time)).unwrap_or(Duration::ZERO);
            *pending_packet = None;
            *pending_frame = None;
            *draining = false;
            *discontinuity = requires_discontinuity;
            *minimum_pts = (mode == SeekMode::Accurate).then_some(seek_position);
            *ended = false;
            *clock = PresentationClock::new(seek_position);
            *playing = shared_timeline.lock_unpoisoned().playing;
            shared_timeline
                .lock_unpoisoned()
                .begin_seek(seek_position, *playing, Instant::now());
            if let Some(audio) = audio {
                audio.seek(seek_time, seek_position)?;
            }
            *handled_seek_generation = generation;
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
    decoder: &mut H264Decoder,
    pending_packet: &mut Option<EncodedVideoPacket>,
    pending_frame: &mut Option<DecodedVideoFrame>,
    draining: &mut bool,
) {
    decoder.flush();
    *pending_packet = None;
    *pending_frame = None;
    *draining = false;
}

enum VideoDecodeStep {
    Progress,
    Frame(DecodedVideoFrame),
    EndOfStream,
}

fn decode_video_step(
    decoder: &mut H264Decoder,
    cursor: &mut PacketCursor<'_, File>,
    seek_controller: &mut H264Mp4SeekController,
    pending_packet: &mut Option<EncodedVideoPacket>,
    draining: &mut bool,
    discontinuity: &mut bool,
    first_description: usize,
) -> MediaResult<VideoDecodeStep> {
    match decoder
        .receive_frame()
        .map_err(|error| decv_decode_error("decv failed while receiving a frame", error))?
    {
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
                        decoder
                            .drain()
                            .map_err(|error| decv_decode_error("decv failed to drain", error))?;
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

            match decoder.send_packet(packet).map_err(|error| {
                decv_decode_error("decv failed while decoding an MP4 packet", error)
            })? {
                DecodeInputStatus::Accepted => {
                    let next_sample = cursor.next_sample_index();
                    if let Err(error) = seek_controller.capture_checkpoint(decoder, cursor) {
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
    cursor: AudioPacketCursor<'demuxer, File>,
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
    fn open(demuxer: &'demuxer Mp4Demuxer<File>, track_index: usize) -> MediaResult<Self> {
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
    if frame.format.pixel_format != PixelFormat::Nv12 {
        return Err(MediaError::new(
            MediaErrorKind::UnsupportedCodec,
            format!(
                "decv produced unsupported pixel format {:?}; gpui_media's decv adapter currently accepts NV12",
                frame.format.pixel_format
            ),
            MediaRecovery::None,
        ));
    }
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
    let [y, uv] = cpu.planes.as_slice() else {
        return Err(decv_decode_message(
            "decv NV12 frame does not contain exactly two planes",
        ));
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
    let surface = SurfaceFrame::new(
        handle,
        sequence,
        coded_size,
        visible_rect,
        display_size,
        SurfaceFormat::Nv12,
        [
            SurfacePlane::with_offset(
                y.bytes.clone(),
                y.offset,
                u32::try_from(y.stride)
                    .map_err(|error| decv_video_output_error("decv Y stride exceeds u32", error))?,
            ),
            SurfacePlane::with_offset(
                uv.bytes.clone(),
                uv.offset,
                u32::try_from(uv.stride).map_err(|error| {
                    decv_video_output_error("decv UV stride exceeds u32", error)
                })?,
            ),
        ],
        surface_color_info(frame.format.color, frame.format.coded_size.height),
    )
    .map_err(|error| decv_video_output_error("GPUI rejected the decv NV12 frame", error))?;

    Ok(VideoFrame::new(
        Arc::new(surface),
        media_time_to_duration(frame.pts),
        media_time_to_duration(frame.duration),
    ))
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

fn local_path(source: &MediaSource) -> MediaResult<PathBuf> {
    let uri = url::Url::parse(source.uri()).map_err(|error| {
        MediaError::from_error(
            MediaErrorKind::InvalidInput,
            MediaRecovery::None,
            "invalid media URI",
            error,
        )
    })?;
    if uri.scheme() != "file" {
        return Err(MediaError::new(
            MediaErrorKind::UnsupportedOperation,
            format!(
                "decv currently accepts local MP4 files only; unsupported URI scheme {:?}",
                uri.scheme()
            ),
            MediaRecovery::None,
        ));
    }
    uri.to_file_path()
        .map_err(|_| MediaError::invalid_input("cannot convert media URI to a local path"))
}

fn video_track_index(demuxer: &Mp4Demuxer<File>) -> MediaResult<usize> {
    demuxer
        .movie()
        .tracks()
        .iter()
        .enumerate()
        .find(|(_, track)| track.kind() == TrackKind::Video && !track.samples().is_empty())
        .map(|(index, _)| index)
        .ok_or_else(|| decv_container_message("MP4 contains no non-empty video track"))
}

fn audio_track_index(demuxer: &Mp4Demuxer<File>) -> Option<usize> {
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
    cursor: &PacketCursor<'_, File>,
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
        Command, DecvBackend, DecvBackendOptions, DecvParallelism, TimelineState, audio_clock_wait,
        coalesce_seek_command, seek_generation_is_current,
    };
    use std::{
        num::NonZeroUsize,
        sync::mpsc,
        time::{Duration, Instant},
    };

    use crate::SeekMode;

    #[test]
    fn backend_parallelism_defaults_to_auto_and_is_configurable() {
        assert_eq!(
            DecvBackend::default().options().selected_parallelism(),
            DecvParallelism::Auto
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
}
