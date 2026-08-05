use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use symphonia::core::{
    audio::sample::Sample,
    codecs::audio::{AudioDecoder, AudioDecoderOptions},
    errors::Error as SymphoniaError,
    formats::{FormatOptions, FormatReader, SeekMode as SymphoniaSeekMode, SeekTo, TrackType},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    units::{Time as SymphoniaTime, Timestamp},
};

use super::{
    output::{PcmOutput, PcmOutputControl},
    player::AudioInfo,
    source::AudioSource,
};
use crate::{MediaError, MediaErrorKind, MediaRecovery, MediaResult, PlaybackTimeline, SeekMode};

const AUDIO_PREROLL_TARGET: Duration = Duration::from_millis(200);
const AUDIO_BUFFER_TARGET: Duration = Duration::from_millis(500);
const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(5);

pub(super) enum AudioWorkerEvent {
    Ready(AudioInfo),
    Ended,
    Error(Arc<MediaError>),
}

enum AudioCommand {
    Seek(Duration, SeekMode),
    Shutdown,
}

struct DesiredState {
    playing: AtomicBool,
    volume: AtomicU64,
    muted: AtomicBool,
    ready: AtomicBool,
    shutdown: AtomicBool,
    seekable: AtomicBool,
}

impl DesiredState {
    fn new(playing: bool, volume: f64, muted: bool) -> Self {
        Self {
            playing: AtomicBool::new(playing),
            volume: AtomicU64::new(volume.to_bits()),
            muted: AtomicBool::new(muted),
            ready: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            seekable: AtomicBool::new(false),
        }
    }
}

pub(super) struct AudioSession {
    source: AudioSource,
    commands: mpsc::Sender<AudioCommand>,
    desired: Arc<DesiredState>,
    output: Arc<Mutex<Option<PcmOutputControl>>>,
    timeline: Arc<Mutex<PlaybackTimeline>>,
    worker: Option<JoinHandle<()>>,
}

impl AudioSession {
    pub(super) fn open(
        source: AudioSource,
        autoplay: bool,
        volume: f64,
        muted: bool,
        events: async_channel::Sender<AudioWorkerEvent>,
    ) -> MediaResult<Self> {
        let desired = Arc::new(DesiredState::new(autoplay, volume, muted));
        let output = Arc::new(Mutex::new(None));
        let timeline = Arc::new(Mutex::new(PlaybackTimeline::default()));
        let (commands, command_receiver) = mpsc::channel();
        let worker_source = source.clone();
        let worker_desired = desired.clone();
        let worker_output = output.clone();
        let worker_timeline = timeline.clone();
        let worker = thread::Builder::new()
            .name("gpui-audio-symphonia".to_owned())
            .spawn(move || {
                if let Err(error) = playback_worker(
                    worker_source,
                    command_receiver,
                    &worker_desired,
                    &worker_output,
                    &worker_timeline,
                    &events,
                ) {
                    if !worker_desired.shutdown.load(Ordering::Acquire) {
                        let _ = events.send_blocking(AudioWorkerEvent::Error(Arc::new(error)));
                    }
                }
            })
            .map_err(|error| MediaError::io("failed to start the audio decoder thread", error))?;
        Ok(Self {
            source,
            commands,
            desired,
            output,
            timeline,
            worker: Some(worker),
        })
    }

    pub(super) fn play(&self) {
        self.desired.playing.store(true, Ordering::Release);
        self.apply_playing();
    }

    pub(super) fn pause(&self) {
        self.desired.playing.store(false, Ordering::Release);
        self.apply_playing();
    }

    pub(super) fn set_volume(&self, volume: f64) {
        self.desired
            .volume
            .store(volume.to_bits(), Ordering::Release);
        if let Some(output) = self.output() {
            output.set_volume(volume);
        }
    }

    pub(super) fn set_muted(&self, muted: bool) {
        self.desired.muted.store(muted, Ordering::Release);
        if let Some(output) = self.output() {
            output.set_muted(muted);
        }
    }

    pub(super) fn seek_to(&self, position: Duration, mode: SeekMode) -> MediaResult<()> {
        if !self.desired.seekable.load(Ordering::Acquire) {
            return Err(MediaError::unsupported("this audio source is not seekable"));
        }
        self.commands
            .send(AudioCommand::Seek(position, mode))
            .map_err(|_| MediaError::backend("audio playback worker has stopped"))
    }

    pub(super) fn timeline(&self) -> PlaybackTimeline {
        *self
            .timeline
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    pub(super) fn is_seekable(&self) -> bool {
        self.desired.seekable.load(Ordering::Acquire)
    }

    fn apply_playing(&self) {
        if let Some(output) = self.output() {
            output.set_playing(
                self.desired.ready.load(Ordering::Acquire)
                    && self.desired.playing.load(Ordering::Acquire),
            );
        }
    }

    fn output(&self) -> Option<PcmOutputControl> {
        self.output
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

impl Drop for AudioSession {
    fn drop(&mut self) {
        self.desired.shutdown.store(true, Ordering::Release);
        self.source.cancel();
        let _ = self.commands.send(AudioCommand::Shutdown);
        self.worker.take();
    }
}

fn playback_worker(
    source: AudioSource,
    commands: mpsc::Receiver<AudioCommand>,
    desired: &Arc<DesiredState>,
    shared_output: &Arc<Mutex<Option<PcmOutputControl>>>,
    shared_timeline: &Arc<Mutex<PlaybackTimeline>>,
    events: &async_channel::Sender<AudioWorkerEvent>,
) -> MediaResult<()> {
    let opened = source.open()?;
    desired.seekable.store(opened.seekable, Ordering::Release);
    let stream = MediaSourceStream::new(opened.source, MediaSourceStreamOptions::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &opened.hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| symphonia_error("failed to detect the audio container", error))?;
    let track = format
        .default_track(TrackType::Audio)
        .cloned()
        .ok_or_else(|| {
            MediaError::new(
                MediaErrorKind::UnsupportedContainer,
                "audio source contains no decodable audio track",
                MediaRecovery::None,
            )
        })?;
    let codec_parameters = track
        .codec_params
        .as_ref()
        .and_then(|parameters| parameters.audio())
        .ok_or_else(|| {
            MediaError::new(
                MediaErrorKind::UnsupportedCodec,
                "audio track has no decoder configuration",
                MediaRecovery::None,
            )
        })?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_parameters, &AudioDecoderOptions::default())
        .map_err(|error| symphonia_error("failed to create the audio decoder", error))?;
    let codec = decoder.codec_info().long_name;
    let track_id = track.id;
    let duration = track
        .time_base
        .zip(track.duration)
        .and_then(|(time_base, duration)| {
            i64::try_from(duration.get())
                .ok()
                .and_then(|duration| time_base.calc_time(Timestamp::new(duration)))
        })
        .and_then(std_duration);
    *shared_timeline
        .lock()
        .unwrap_or_else(|error| error.into_inner()) =
        PlaybackTimeline::new(Duration::ZERO, duration, opened.seekable);

    let mut output = None;
    let mut samples = Vec::new();
    let mut end_of_stream = false;
    let mut ended_emitted = false;
    loop {
        if desired.shutdown.load(Ordering::Acquire) {
            return Ok(());
        }
        while let Ok(command) = commands.try_recv() {
            match command {
                AudioCommand::Seek(position, mode) => {
                    seek(
                        &mut *format,
                        &mut *decoder,
                        output.as_mut(),
                        position,
                        mode,
                        track_id,
                    )?;
                    desired.ready.store(false, Ordering::Release);
                    if let Some(output) = output.as_ref() {
                        output.control().set_playing(false);
                    }
                    *shared_timeline
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) =
                        PlaybackTimeline::new(position, duration, opened.seekable);
                    end_of_stream = false;
                    ended_emitted = false;
                }
                AudioCommand::Shutdown => return Ok(()),
            }
        }

        if let Some(output) = output.as_ref() {
            output.check_error()?;
            update_timeline(
                output.position(),
                duration,
                opened.seekable,
                shared_timeline,
            );
        }

        if end_of_stream {
            {
                let Some(output) = output.as_ref() else {
                    return Err(MediaError::new(
                        MediaErrorKind::Decode,
                        "audio stream ended before producing decoded samples",
                        MediaRecovery::None,
                    ));
                };
                if !desired.ready.swap(true, Ordering::AcqRel) {
                    apply_desired_output(output, desired);
                    let info = audio_info(output, duration, opened.seekable, codec);
                    let _ = events.send_blocking(AudioWorkerEvent::Ready(info));
                }
                if output.is_drained() && !ended_emitted {
                    ended_emitted = true;
                    desired.playing.store(false, Ordering::Release);
                    output.control().set_playing(false);
                    let position = duration.unwrap_or_else(|| output.position());
                    update_timeline(position, duration, opened.seekable, shared_timeline);
                    let _ = events.send_blocking(AudioWorkerEvent::Ended);
                }
            }
            match commands.recv_timeout(WORKER_POLL_INTERVAL) {
                Ok(AudioCommand::Seek(position, mode)) => {
                    seek(
                        &mut *format,
                        &mut *decoder,
                        output.as_mut(),
                        position,
                        mode,
                        track_id,
                    )?;
                    desired.ready.store(false, Ordering::Release);
                    if let Some(output) = output.as_ref() {
                        output.control().set_playing(false);
                    }
                    end_of_stream = false;
                    ended_emitted = false;
                }
                Ok(AudioCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Ok(());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            continue;
        }

        let target = if desired.ready.load(Ordering::Acquire) {
            AUDIO_BUFFER_TARGET
        } else {
            AUDIO_PREROLL_TARGET
        };
        if output
            .as_ref()
            .is_some_and(|output| output.buffered_duration() >= target)
        {
            if !desired.ready.swap(true, Ordering::AcqRel) {
                let output = output.as_ref().expect("audio output exists after preroll");
                apply_desired_output(output, desired);
                let info = audio_info(output, duration, opened.seekable, codec);
                let _ = events.send_blocking(AudioWorkerEvent::Ready(info));
            }
            thread::sleep(WORKER_POLL_INTERVAL);
            continue;
        }

        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => {
                if let Some(output) = output.as_mut() {
                    output.flush();
                }
                end_of_stream = true;
                continue;
            }
            Err(SymphoniaError::DecodeError(error)) => {
                log::debug!("symphonia skipped malformed audio packet: {error}");
                continue;
            }
            Err(error) => return Err(symphonia_error("failed to read an audio packet", error)),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(error)) => {
                log::debug!("symphonia skipped undecodable audio packet: {error}");
                continue;
            }
            Err(error) => return Err(symphonia_error("failed to decode audio", error)),
        };
        let channels = u16::try_from(decoded.spec().channels().count()).map_err(|_| {
            MediaError::new(
                MediaErrorKind::Decode,
                "decoded audio channel count exceeds the supported range",
                MediaRecovery::None,
            )
        })?;
        let sample_rate = decoded.spec().rate();
        if let Some(output) = output.as_ref()
            && !output.matches_format(channels, sample_rate)
        {
            return Err(MediaError::new(
                MediaErrorKind::AudioOutput,
                "mid-stream audio format changes are not supported",
                MediaRecovery::None,
            ));
        }
        if output.is_none() {
            let opened_output = PcmOutput::open(channels, sample_rate)?;
            opened_output.set_volume(f64::from_bits(desired.volume.load(Ordering::Acquire)));
            opened_output.set_muted(desired.muted.load(Ordering::Acquire));
            opened_output.start()?;
            *shared_output
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(opened_output.control());
            output = Some(opened_output);
        }
        samples.resize(decoded.samples_interleaved(), f32::MID);
        decoded.copy_to_slice_interleaved(&mut samples);
        output
            .as_mut()
            .expect("audio output was opened for the decoded format")
            .enqueue(&samples);
    }
}

fn seek(
    format: &mut dyn FormatReader,
    decoder: &mut dyn AudioDecoder,
    output: Option<&mut PcmOutput>,
    position: Duration,
    mode: SeekMode,
    track_id: u32,
) -> MediaResult<()> {
    let time = SymphoniaTime::try_from_nanos_u128(position.as_nanos()).ok_or_else(|| {
        MediaError::invalid_input("audio seek position exceeds the supported range")
    })?;
    format
        .seek(
            match mode {
                SeekMode::Accurate => SymphoniaSeekMode::Accurate,
                SeekMode::Interactive | SeekMode::KeyFrame => SymphoniaSeekMode::Coarse,
            },
            SeekTo::Time {
                time,
                track_id: Some(track_id),
            },
        )
        .map_err(|error| symphonia_error("failed to seek the audio stream", error))?;
    decoder.reset();
    if let Some(output) = output {
        output.reset(position);
    }
    Ok(())
}

fn apply_desired_output(output: &PcmOutput, desired: &DesiredState) {
    let control = output.control();
    control.set_volume(f64::from_bits(desired.volume.load(Ordering::Acquire)));
    control.set_muted(desired.muted.load(Ordering::Acquire));
    control.set_playing(desired.playing.load(Ordering::Acquire));
}

fn audio_info(
    output: &PcmOutput,
    duration: Option<Duration>,
    seekable: bool,
    codec: &'static str,
) -> AudioInfo {
    AudioInfo {
        sample_rate: output.source_sample_rate(),
        channels: output.source_channels(),
        duration,
        seekable,
        codec: codec.into(),
    }
}

fn update_timeline(
    position: Duration,
    duration: Option<Duration>,
    seekable: bool,
    timeline: &Mutex<PlaybackTimeline>,
) {
    *timeline.lock().unwrap_or_else(|error| error.into_inner()) =
        PlaybackTimeline::new(position, duration, seekable);
}

fn std_duration(time: symphonia::core::units::Time) -> Option<Duration> {
    let nanos = u64::try_from(time.as_nanos()).ok()?;
    Some(Duration::from_nanos(nanos))
}

fn symphonia_error(context: impl std::fmt::Display, error: SymphoniaError) -> MediaError {
    let kind = match error {
        SymphoniaError::IoError(_) => MediaErrorKind::Io {
            kind: std::io::ErrorKind::Other,
        },
        SymphoniaError::Unsupported(_) => MediaErrorKind::UnsupportedCodec,
        SymphoniaError::SeekError(_) => MediaErrorKind::UnsupportedOperation,
        _ => MediaErrorKind::Decode,
    };
    MediaError::from_error(kind, MediaRecovery::None, context, error)
}
