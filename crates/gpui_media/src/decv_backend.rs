use std::{
    fs::File,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use decv_core::{
    ColorMatrix as DecvColorMatrix, ColorRange as DecvColorRange, DecodeInputStatus, DecodeOutput,
    DecodedVideoFrame, EncodedVideoPacket, FrameStorage, MediaTime, PixelFormat, VideoDecoder,
};
use decv_h264::{H264Decoder, H264Parallelism};
use decv_mp4::{FourCc, Mp4Demuxer, PacketCursor};
use gpui::{
    Bounds, ColorRange, DevicePixels, SurfaceColorInfo, SurfaceFormat, SurfaceFrame, SurfaceHandle,
    SurfacePlane, YuvMatrix, point, size,
};

use crate::{
    MediaBackend, MediaBackendEvent, MediaCapabilities, MediaError, MediaErrorKind,
    MediaOutputSink, MediaPlaybackRequest, MediaPlaybackSession, MediaRecovery, MediaResult,
    MediaSource, PlaybackTimeline, SeekMode, VideoFrame,
};

const VIDEO_HANDLER: FourCc = FourCc::new(*b"vide");
const MEDIA_TIME_SCALE: u32 = 1_000_000_000;

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

/// Pure-Rust MP4/H.264 playback backed by `decv`.
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
        let (commands, receiver) = mpsc::channel();
        let worker_timeline = timeline.clone();
        let worker_path = path;
        let worker = thread::Builder::new()
            .name("gpui-video-decv".to_owned())
            .spawn(move || {
                if let Err(error) = playback_worker(
                    &worker_path,
                    probe.track_index,
                    receiver,
                    &worker_timeline,
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
        self.timeline
            .lock_unpoisoned()
            .reset(Duration::ZERO, autoplay, Instant::now());
        self.send(Command::Reload { autoplay })
    }

    fn seek_to(&mut self, position: Duration, mode: SeekMode) -> MediaResult<()> {
        let mut timeline = self.timeline.lock_unpoisoned();
        let target = timeline
            .duration
            .map_or(position, |duration| position.min(duration));
        let playing = timeline.playing;
        timeline.reset(target, playing, Instant::now());
        drop(timeline);
        self.send(Command::Seek {
            position: target,
            mode,
        })
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
    Reload { autoplay: bool },
    Seek { position: Duration, mode: SeekMode },
    Shutdown,
}

struct Probe {
    track_index: usize,
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
    let track_index = video_track_index(&demuxer)?;
    let duration = demuxer
        .movie()
        .duration()
        .map(|value| duration_from_ratio(value, demuxer.movie().timescale().get()))
        .transpose()?;
    Ok(Probe {
        track_index,
        duration,
    })
}

fn playback_worker(
    path: &Path,
    track_index: usize,
    commands: mpsc::Receiver<Command>,
    shared_timeline: &Mutex<TimelineState>,
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
    if track_index >= demuxer.movie().tracks().len() {
        return Err(decv_container_message(
            "video track changed after probing the MP4",
        ));
    }
    let mut cursor = demuxer
        .packet_cursor(track_index)
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

    let surface_handle = SurfaceHandle::new();
    let mut sequence = 0u64;
    let mut pending_packet = None;
    let mut pending_frame = None;
    let mut draining = false;
    let mut playing = false;
    let mut preroll = false;
    let mut ended = false;
    let mut minimum_pts = None;
    let mut clock = PresentationClock::new(Duration::ZERO);

    loop {
        if !playing && !preroll {
            let command = commands
                .recv()
                .map_err(|_| MediaError::backend("decv playback command channel closed"))?;
            if apply_command(
                command,
                &mut cursor,
                &mut decoder,
                &mut pending_packet,
                &mut pending_frame,
                &mut draining,
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

        if let Ok(command) = commands.try_recv() {
            if apply_command(
                command,
                &mut cursor,
                &mut decoder,
                &mut pending_packet,
                &mut pending_frame,
                &mut draining,
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

        if pending_frame.is_none() {
            pending_frame = decode_next_frame(
                &mut decoder,
                &mut cursor,
                &mut pending_packet,
                &mut draining,
                first_description,
            )?;
            if pending_frame.is_none() {
                if !ended {
                    ended = true;
                    playing = false;
                    let now = Instant::now();
                    shared_timeline.lock_unpoisoned().finish(now);
                    output.emit(MediaBackendEvent::Ended);
                }
                continue;
            }
            if let Ok(command) = commands.try_recv() {
                if apply_command(
                    command,
                    &mut cursor,
                    &mut decoder,
                    &mut pending_packet,
                    &mut pending_frame,
                    &mut draining,
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
        let deadline = clock.deadline(timestamp);
        let now = Instant::now();
        if deadline > now {
            match commands.recv_timeout(deadline.duration_since(now)) {
                Ok(command) => {
                    if apply_command(
                        command,
                        &mut cursor,
                        &mut decoder,
                        &mut pending_packet,
                        &mut pending_frame,
                        &mut draining,
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
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }

        let frame = pending_frame
            .take()
            .expect("the scheduled frame remains pending");
        let completed_preroll = preroll;
        sequence = sequence.wrapping_add(1);
        let frame = Arc::new(convert_frame(frame, surface_handle.clone(), sequence)?);
        let timestamp = frame.timestamp().unwrap_or(timestamp);
        shared_timeline
            .lock_unpoisoned()
            .present(timestamp, Instant::now());
        output.publish_video_frame(frame);
        preroll = false;
        if completed_preroll {
            output.emit(MediaBackendEvent::Ready);
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
    pending_packet: &mut Option<EncodedVideoPacket>,
    pending_frame: &mut Option<DecodedVideoFrame>,
    draining: &mut bool,
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
            *playing = true;
            *preroll = false;
        }
        Command::Pause => {
            *playing = false;
            *preroll = pending_frame.is_none() && cursor.next_sample_index() == 0 && !*ended;
        }
        Command::Reload { autoplay } => {
            cursor.rewind();
            reset_decode_timeline(decoder, pending_packet, pending_frame, draining);
            *minimum_pts = None;
            *ended = false;
            *playing = autoplay;
            *preroll = !autoplay;
            *clock = PresentationClock::new(Duration::ZERO);
        }
        Command::Seek { position, mode } => {
            let target = duration_to_media_time(position)?;
            cursor
                .seek_to_keyframe(target)
                .map_err(|error| decv_container_error("failed to seek MP4 packet cursor", error))?
                .ok_or_else(|| {
                    MediaError::new(
                        MediaErrorKind::UnsupportedOperation,
                        format!("no MP4 keyframe exists at or before {position:?}"),
                        MediaRecovery::None,
                    )
                })?;
            reset_decode_timeline(decoder, pending_packet, pending_frame, draining);
            *minimum_pts = (mode == SeekMode::Accurate).then_some(position);
            *ended = false;
            *clock = PresentationClock::new(position);
            *playing = shared_timeline.lock_unpoisoned().playing;
            *preroll = true;
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

fn decode_next_frame(
    decoder: &mut H264Decoder,
    cursor: &mut PacketCursor<'_, File>,
    pending_packet: &mut Option<EncodedVideoPacket>,
    draining: &mut bool,
    first_description: usize,
) -> MediaResult<Option<DecodedVideoFrame>> {
    loop {
        match decoder
            .receive_frame()
            .map_err(|error| decv_decode_error("decv failed while receiving a frame", error))?
        {
            DecodeOutput::Frame(frame) => return Ok(Some(frame)),
            DecodeOutput::FormatChanged(format) => {
                log::debug!(
                    "decv format changed: {}x{} {:?}",
                    format.coded_size.width,
                    format.coded_size.height,
                    format.pixel_format
                );
            }
            DecodeOutput::EndOfStream => return Ok(None),
            DecodeOutput::NeedInput => {
                if *draining {
                    return Err(decv_decode_message("decv requested input after drain"));
                }

                let packet = match pending_packet.take() {
                    Some(packet) => packet,
                    None => {
                        let Some(packet) = cursor.next_packet().map_err(|error| {
                            decv_container_error("failed to read the next MP4 packet", error)
                        })?
                        else {
                            decoder.drain().map_err(|error| {
                                decv_decode_error("decv failed to drain", error)
                            })?;
                            *draining = true;
                            continue;
                        };
                        let sample = &cursor.track().samples()[cursor.next_sample_index() - 1];
                        if sample.description_index() != first_description {
                            return Err(MediaError::new(
                                MediaErrorKind::UnsupportedCodec,
                                "mid-stream MP4 sample-description changes are not supported",
                                MediaRecovery::None,
                            ));
                        }
                        packet
                    }
                };

                match decoder.send_packet(packet).map_err(|error| {
                    decv_decode_error("decv failed while decoding an MP4 packet", error)
                })? {
                    DecodeInputStatus::Accepted => {}
                    DecodeInputStatus::NeedOutput(unconsumed) => {
                        *pending_packet = Some(unconsumed);
                    }
                    status => {
                        return Err(decv_decode_message(format!(
                            "decv returned an unsupported input status: {status:?}"
                        )));
                    }
                }
            }
            output => {
                return Err(decv_decode_message(format!(
                    "decv returned an unsupported decoder output: {output:?}"
                )));
            }
        }
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

fn surface_color_info(color: decv_core::ColorInfo, coded_height: u32) -> SurfaceColorInfo {
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
        .find(|(_, track)| track.handler() == VIDEO_HANDLER && !track.samples().is_empty())
        .map(|(index, _)| index)
        .ok_or_else(|| decv_container_message("MP4 contains no non-empty video track"))
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

fn device_size(value: decv_core::Size) -> MediaResult<gpui::Size<DevicePixels>> {
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
}

impl TimelineState {
    fn new(duration: Option<Duration>) -> Self {
        Self {
            position: Duration::ZERO,
            duration,
            playing: false,
            wall_anchor: Instant::now(),
        }
    }

    fn current_position(&self, now: Instant) -> Duration {
        let position = if self.playing {
            self.position
                .saturating_add(now.saturating_duration_since(self.wall_anchor))
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

    fn reset(&mut self, position: Duration, playing: bool, now: Instant) {
        self.position = self
            .duration
            .map_or(position, |duration| position.min(duration));
        self.playing = playing;
        self.wall_anchor = now;
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
    use super::{DecvBackend, DecvBackendOptions, DecvParallelism};
    use std::num::NonZeroUsize;

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
}
