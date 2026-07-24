use std::{
    fs::File,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, anyhow, bail};
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
    BackendCapabilities, BackendEvent, MediaSource, PlaybackBackendRequest, PlaybackOutputSink,
    PlaybackSession, PlaybackTimeline, SeekMode, VideoBackend, VideoFrame,
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
/// `gpui_video`'s stable backend boundary.
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

impl VideoBackend for DecvBackend {
    fn name(&self) -> &'static str {
        "decv"
    }

    fn open_playback(
        &self,
        request: PlaybackBackendRequest,
        output: PlaybackOutputSink,
    ) -> Result<Box<dyn PlaybackSession>> {
        Ok(Box::new(DecvSession::open(
            request.source,
            output,
            self.options,
        )?))
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
        output: PlaybackOutputSink,
        options: DecvBackendOptions,
    ) -> Result<Self> {
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
                        "decv playback failed for {}: {error:#}",
                        worker_path.display()
                    );
                    output.emit(BackendEvent::Error(error.to_string().into()));
                }
            })
            .context("failed to start decv playback thread")?;

        Ok(Self {
            commands,
            timeline,
            worker: Some(worker),
        })
    }

    fn send(&self, command: Command) -> Result<()> {
        self.commands
            .send(command)
            .map_err(|_| anyhow!("decv playback thread has stopped"))
    }
}

impl PlaybackSession for DecvSession {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            seeking: true,
            accurate_seeking: true,
            ..BackendCapabilities::default()
        }
    }

    fn play(&mut self) -> Result<()> {
        self.timeline
            .lock_unpoisoned()
            .set_playing(true, Instant::now());
        self.send(Command::Play)
    }

    fn pause(&mut self) -> Result<()> {
        self.timeline
            .lock_unpoisoned()
            .set_playing(false, Instant::now());
        self.send(Command::Pause)
    }

    fn timeline(&self) -> PlaybackTimeline {
        self.timeline.lock_unpoisoned().snapshot(Instant::now())
    }

    fn reload(&mut self, autoplay: bool) -> Result<()> {
        self.timeline
            .lock_unpoisoned()
            .reset(Duration::ZERO, autoplay, Instant::now());
        self.send(Command::Reload { autoplay })
    }

    fn seek_to(&mut self, position: Duration, mode: SeekMode) -> Result<()> {
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

fn probe_mp4(path: &Path) -> Result<Probe> {
    let demuxer = Mp4Demuxer::open(
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse MP4 {}", path.display()))?;
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
    output: &PlaybackOutputSink,
    parallelism: DecvParallelism,
) -> Result<()> {
    let demuxer = Mp4Demuxer::open(
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?,
    )?;
    if track_index >= demuxer.movie().tracks().len() {
        bail!("video track changed after probing the MP4");
    }
    let mut cursor = demuxer.packet_cursor(track_index)?;
    let decoder_config = cursor
        .decoder_config()?
        .context("MP4 video track has no decoder configuration")?;
    let first_description = cursor
        .track()
        .samples()
        .first()
        .context("MP4 video track has no samples")?
        .description_index();

    let mut decoder = H264Decoder::new();
    decoder
        .set_parallelism(parallelism.into())
        .context("failed to configure decv H.264 parallelism")?;
    decoder
        .configure(decoder_config)
        .context("failed to configure decv H.264 decoder")?;

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
                .map_err(|_| anyhow!("decv playback command channel closed"))?;
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
                    output.emit(BackendEvent::Ended);
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
        sequence = sequence.wrapping_add(1);
        let frame = Arc::new(convert_frame(frame, surface_handle.clone(), sequence)?);
        let timestamp = frame.timestamp().unwrap_or(timestamp);
        shared_timeline
            .lock_unpoisoned()
            .present(timestamp, Instant::now());
        output.publish_frame(frame);
        preroll = false;
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
) -> Result<bool> {
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
                .seek_to_keyframe(target)?
                .with_context(|| format!("no MP4 keyframe exists at or before {position:?}"))?;
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
) -> Result<Option<DecodedVideoFrame>> {
    loop {
        match decoder
            .receive_frame()
            .context("decv failed while receiving a frame")?
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
                    bail!("decv requested input after drain");
                }

                let packet = match pending_packet.take() {
                    Some(packet) => packet,
                    None => {
                        let Some(packet) = cursor.next_packet()? else {
                            decoder.drain().context("decv failed to drain")?;
                            *draining = true;
                            continue;
                        };
                        let sample = &cursor.track().samples()[cursor.next_sample_index() - 1];
                        if sample.description_index() != first_description {
                            bail!("mid-stream MP4 sample-description changes are not supported");
                        }
                        packet
                    }
                };

                match decoder
                    .send_packet(packet)
                    .context("decv failed while decoding an MP4 packet")?
                {
                    DecodeInputStatus::Accepted => {}
                    DecodeInputStatus::NeedOutput(unconsumed) => {
                        *pending_packet = Some(unconsumed);
                    }
                    status => bail!("decv returned an unsupported input status: {status:?}"),
                }
            }
            output => bail!("decv returned an unsupported decoder output: {output:?}"),
        }
    }
}

fn convert_frame(
    frame: DecodedVideoFrame,
    handle: SurfaceHandle,
    sequence: u64,
) -> Result<VideoFrame> {
    frame.validate().context("decv produced an invalid frame")?;
    if frame.format.pixel_format != PixelFormat::Nv12 {
        bail!(
            "decv produced unsupported pixel format {:?}; gpui_video's decv adapter currently accepts NV12",
            frame.format.pixel_format
        );
    }
    let cpu = match frame.storage {
        FrameStorage::Cpu(cpu) => cpu,
        _ => bail!("decv produced a non-CPU frame that this adapter does not support"),
    };
    let [y, uv] = cpu.planes.as_slice() else {
        bail!("decv NV12 frame does not contain exactly two planes");
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
                u32::try_from(y.stride).context("decv Y stride exceeds u32")?,
            ),
            SurfacePlane::with_offset(
                uv.bytes.clone(),
                uv.offset,
                u32::try_from(uv.stride).context("decv UV stride exceeds u32")?,
            ),
        ],
        surface_color_info(frame.format.color, frame.format.coded_size.height),
    )
    .context("GPUI rejected the decv NV12 frame")?;

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

fn local_path(source: &MediaSource) -> Result<PathBuf> {
    let uri = url::Url::parse(source.uri()).context("invalid media URI")?;
    if uri.scheme() != "file" {
        bail!(
            "decv currently accepts local MP4 files only; unsupported URI scheme {:?}",
            uri.scheme()
        );
    }
    uri.to_file_path()
        .map_err(|_| anyhow!("cannot convert media URI to a local path"))
}

fn video_track_index(demuxer: &Mp4Demuxer<File>) -> Result<usize> {
    demuxer
        .movie()
        .tracks()
        .iter()
        .enumerate()
        .find(|(_, track)| track.handler() == VIDEO_HANDLER && !track.samples().is_empty())
        .map(|(index, _)| index)
        .context("MP4 contains no non-empty video track")
}

fn duration_from_ratio(value: u64, timescale: u32) -> Result<Duration> {
    if timescale == 0 {
        bail!("media timescale is zero");
    }
    let seconds = value / u64::from(timescale);
    let remainder = value % u64::from(timescale);
    let nanos = (u128::from(remainder) * 1_000_000_000u128) / u128::from(timescale);
    Ok(Duration::new(
        seconds,
        u32::try_from(nanos).context("media timestamp exceeds nanosecond precision")?,
    ))
}

fn media_time_to_duration(time: Option<MediaTime>) -> Option<Duration> {
    let time = time?;
    if time.value < 0 {
        return None;
    }
    duration_from_ratio(time.value as u64, time.timescale.get()).ok()
}

fn duration_to_media_time(duration: Duration) -> Result<MediaTime> {
    let value = i64::try_from(duration.as_nanos())
        .context("seek target cannot be represented by decv MediaTime")?;
    MediaTime::from_parts(value, MEDIA_TIME_SCALE).context("invalid decv media timescale")
}

fn device_size(value: decv_core::Size) -> Result<gpui::Size<DevicePixels>> {
    Ok(size(
        device_pixel(value.width)?,
        device_pixel(value.height)?,
    ))
}

fn device_pixel(value: u32) -> Result<DevicePixels> {
    Ok(DevicePixels(
        i32::try_from(value).context("video dimension exceeds i32")?,
    ))
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
