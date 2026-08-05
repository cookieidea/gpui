use std::time::Duration;

use gpui::{
    App, AppContext, Bounds, Context, CursorStyle, Entity, IntoElement, Render, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgba, size,
};
use gpui_media::{
    AudioPlayer, AudioPlayerEvent, AudioPlayerOptions, AudioSource, AudioStreamHint,
    AudioStreamWriter, PlaybackState, SeekMode,
};

struct AudioExample {
    player: Entity<AudioPlayer>,
}

impl AudioExample {
    fn new(player: Entity<AudioPlayer>, cx: &mut Context<Self>) -> Self {
        cx.subscribe(&player, |_, _, _, cx| cx.notify()).detach();
        Self { player }
    }

    fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self
            .player
            .update(cx, |player, cx| player.toggle_playback(cx))
        {
            eprintln!("audio playback control failed: {error}");
        }
    }

    fn seek_by(&mut self, delta_seconds: i64, cx: &mut Context<Self>) {
        let player = self.player.read(cx);
        if !player.is_seekable() {
            return;
        }
        let position = if delta_seconds.is_negative() {
            player
                .position()
                .saturating_sub(Duration::from_secs(delta_seconds.unsigned_abs()))
        } else {
            player
                .position()
                .saturating_add(Duration::from_secs(delta_seconds as u64))
        };
        if let Err(error) = self.player.update(cx, |player, cx| {
            player.seek_to(position, SeekMode::Accurate, cx)
        }) {
            eprintln!("audio seek failed: {error}");
        }
    }

    fn toggle_muted(&mut self, cx: &mut Context<Self>) {
        self.player.update(cx, |player, cx| player.toggle_muted(cx));
    }
}

impl Render for AudioExample {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let player = self.player.read(cx);
        let state = format!("{:?}", player.state());
        let position = format_duration(player.position());
        let duration = player
            .duration()
            .map(format_duration)
            .unwrap_or_else(|| "live".to_owned());
        let playing = matches!(
            player.state(),
            PlaybackState::Playing | PlaybackState::Loading
        );
        let muted = player.is_muted();
        let seekable = player.is_seekable();
        let info = player.info().map_or_else(
            || "Waiting for stream metadata".to_owned(),
            |info| {
                format!(
                    "{} · {} Hz · {} channel{}",
                    info.codec(),
                    info.sample_rate(),
                    info.channels(),
                    if info.channels() == 1 { "" } else { "s" }
                )
            },
        );
        let button = |id, label: &'static str, enabled: bool| {
            div()
                .id(id)
                .px_4()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(rgba(0xffffff28))
                .bg(if enabled {
                    rgba(0xffffff14)
                } else {
                    rgba(0xffffff08)
                })
                .text_color(if enabled {
                    rgba(0xffffffff)
                } else {
                    rgba(0xffffff66)
                })
                .when(enabled, |element| element.cursor(CursorStyle::PointingHand))
                .child(label)
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .justify_center()
            .items_center()
            .gap_4()
            .bg(rgba(0x11151cff))
            .text_color(gpui::white())
            .child(div().text_xl().child("gpui_media audio"))
            .child(div().text_color(rgba(0xffffff99)).child(info))
            .child(div().child(format!("{position} / {duration} · {state}")))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        button("audio-backward", "-10s", seekable).when(seekable, |element| {
                            element.on_click(cx.listener(|this, _, _, cx| this.seek_by(-10, cx)))
                        }),
                    )
                    .child(
                        button(
                            "audio-playback-toggle",
                            if playing { "Pause" } else { "Play" },
                            true,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_playback(cx))),
                    )
                    .child(
                        button("audio-forward", "+10s", seekable).when(seekable, |element| {
                            element.on_click(cx.listener(|this, _, _, cx| this.seek_by(10, cx)))
                        }),
                    )
                    .child(
                        button(
                            "audio-mute-toggle",
                            if muted { "Unmute" } else { "Mute" },
                            true,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_muted(cx))),
                    ),
            )
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (source, feed) = parse_input()?;
    let exit_on_end = std::env::var_os("GPUI_AUDIO_EXIT_ON_END").is_some();

    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(620.0), px(280.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |window, cx| {
                window.set_window_title("gpui_media audio");
                let player = cx.new(|cx| {
                    AudioPlayer::new(source, AudioPlayerOptions::default(), cx)
                        .expect("failed to create audio player")
                });
                if let Some((writer, path)) = feed {
                    cx.background_spawn(async move {
                        match std::fs::read(&path) {
                            Ok(bytes) => {
                                for chunk in bytes.chunks(16 * 1024) {
                                    if let Err(error) = writer.write(chunk.to_vec()).await {
                                        eprintln!("failed to feed encoded audio: {error}");
                                        break;
                                    }
                                }
                            }
                            Err(error) => {
                                eprintln!("failed to read {}: {error}", path.display());
                            }
                        }
                        writer.close();
                    })
                    .detach();
                }
                cx.subscribe(&player, move |_, event, cx| {
                    if let AudioPlayerEvent::StateChanged(state) = event {
                        match state {
                            PlaybackState::Ended if exit_on_end => cx.quit(),
                            PlaybackState::Error(error) => {
                                eprintln!("audio playback failed: {error}");
                                if exit_on_end {
                                    cx.quit();
                                }
                            }
                            _ => {}
                        }
                    }
                })
                .detach();
                cx.new(|cx| AudioExample::new(player, cx))
            },
        );
        if let Err(error) = result {
            eprintln!("gpui_media audio example: failed to open window: {error:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });

    Ok(())
}

fn parse_input() -> Result<
    (AudioSource, Option<(AudioStreamWriter, std::path::PathBuf)>),
    Box<dyn std::error::Error>,
> {
    let mut arguments = std::env::args().skip(1);
    let Some(first) = arguments.next() else {
        return Err(invalid_arguments().into());
    };
    if first != "--stream" {
        if arguments.next().is_some() {
            return Err(invalid_arguments().into());
        }
        return Ok((AudioSource::parse(first)?, None));
    }

    let extension = arguments.next().ok_or_else(invalid_arguments)?;
    let path = arguments
        .next()
        .map(std::path::PathBuf::from)
        .ok_or_else(invalid_arguments)?;
    if arguments.next().is_some() {
        return Err(invalid_arguments().into());
    }
    let (writer, source) =
        AudioSource::stream(8, AudioStreamHint::new().with_extension(extension))?;
    Ok((source, Some((writer, path))))
}

fn invalid_arguments() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "usage: cargo run -p gpui_media --example audio -- \
         <audio file or HTTP(S) URL>\n       cargo run -p gpui_media --example audio -- \
         --stream <extension> <encoded audio file>",
    )
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
