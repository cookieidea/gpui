use std::time::Instant;

use anyhow::Result;
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_video::{
    DecvBackend, MediaSource, PlaybackState, VideoPlayer, VideoPlayerEvent, VideoPlayerOptions,
};

fn main() -> Result<()> {
    let input = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!(
            "Usage: cargo run -p gpui_video --no-default-features \
             --features backend-decv --example decv_play -- <local MP4>"
        );
        std::process::exit(2);
    });
    let source = MediaSource::from_path(input)?;
    let title = format!("gpui_video · decv · {}", source.display_name());

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
                        .backend(DecvBackend)
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
            eprintln!("gpui_video decv example: failed to open window: {error:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });

    Ok(())
}
