use std::{num::NonZeroUsize, time::Instant};

use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_media::{
    DecvBackend, DecvParallelism, MediaSource, PlaybackState, VideoPlayer, VideoPlayerEvent,
    VideoPlayerOptions,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (input, parallelism) = parse_arguments()?;
    let source = MediaSource::parse(input)?;
    let title = format!("gpui_media · decv · {}", source.display_name());

    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.0), px(680.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |window, cx| {
                window.set_window_title(&title);
                let player = cx.new(|cx| {
                    VideoPlayer::builder(source)
                        .options(VideoPlayerOptions::default())
                        .backend(DecvBackend::new().parallelism(parallelism))
                        .build_in_window(window, cx)
                        .expect("failed to create decv video player")
                });
                let metrics_player = player.clone();
                let started = Instant::now();
                let exit_on_end = std::env::var_os("GPUI_VIDEO_EXIT_ON_END").is_some();
                cx.subscribe(&player, move |_, event, cx| match event {
                    VideoPlayerEvent::StateChanged(PlaybackState::Ended) => {
                        let (duration, stats) = {
                            let player = metrics_player.read(cx);
                            (player.duration(), player.stats())
                        };
                        eprintln!(
                            "decv playback completed: media={duration:?} wall={:?} \
                             decoded={} delivered={} dropped={}",
                            started.elapsed(),
                            stats.decoded_frames(),
                            stats.delivered_frames(),
                            stats.dropped_frames(),
                        );
                        if exit_on_end {
                            cx.quit();
                        }
                    }
                    VideoPlayerEvent::StateChanged(PlaybackState::Error(error)) => {
                        eprintln!("decv playback failed: {error}");
                        if exit_on_end {
                            cx.quit();
                        }
                    }
                    _ => {}
                })
                .detach();
                player
            },
        );
        if let Err(error) = result {
            eprintln!("gpui_media decv example: failed to open window: {error:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });

    Ok(())
}

fn parse_arguments() -> Result<(String, DecvParallelism), std::io::Error> {
    let mut arguments = std::env::args().skip(1);
    let mut input = None;
    let mut parallelism = DecvParallelism::Auto;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--parallelism" => {
                let value = arguments.next().ok_or_else(|| {
                    invalid_argument("--parallelism requires auto, serial, or a thread count")
                })?;
                parallelism = match value.as_str() {
                    "auto" => DecvParallelism::Auto,
                    "serial" => DecvParallelism::Serial,
                    value => DecvParallelism::Threads(
                        value
                            .parse::<usize>()
                            .ok()
                            .and_then(NonZeroUsize::new)
                            .ok_or_else(|| {
                                invalid_argument(
                                    "--parallelism must be auto, serial, or a positive thread count"
                                )
                            })?,
                    ),
                };
            }
            value if value.starts_with('-') => {
                return Err(invalid_argument(format!("unknown option {value:?}")));
            }
            value if input.is_none() => input = Some(value.to_owned()),
            value => return Err(invalid_argument(format!("unexpected argument {value:?}"))),
        }
    }

    let input = input.ok_or_else(|| {
        invalid_argument(
            "Usage: cargo run -p gpui_media --no-default-features \
             --features backend-decv --example decv_play -- \
             [--parallelism auto|serial|THREADS] <MP4 path or HTTP(S) URL>",
        )
    })?;
    Ok((input, parallelism))
}

fn invalid_argument(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into())
}
