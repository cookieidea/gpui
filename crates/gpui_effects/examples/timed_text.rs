use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, Background, Bounds, Context, FontWeight, Render, Window,
    WindowBounds, WindowOptions, div, linear_color_stop, multi_linear_gradient, point, prelude::*,
    px, rgb, rgba, size,
};
use gpui_effects::{GlassMaterial, GlassPanel, TimedText, TimedTextEmphasis, TimedTextUnit};
use gpui_platform::application;

type Piece = (&'static str, u64, u64, usize);

const CHINESE_LINE: &str = "长音符短长旋律快长回声啪";
const ENGLISH_LINE: &str = "sustain  short  melody  tap  long echo  pop";
const MIXED_LINE: &str = "长音符short长旋律tap long echo快";
const LOOP_SECONDS: f32 = 8.5;

const CHINESE_PIECES: &[Piece] = &[
    ("长音符", 300, 1800, 0),
    ("短", 2050, 2350, 1),
    ("长旋律", 2700, 4300, 2),
    ("快", 4550, 4800, 3),
    ("长回声", 5200, 7200, 4),
    ("啪", 7480, 7720, 5),
];

const ENGLISH_PIECES: &[Piece] = &[
    ("sustain", 300, 1800, 0),
    ("short", 2050, 2350, 1),
    ("melody", 2700, 4300, 2),
    ("tap", 4550, 4800, 3),
    ("long", 5200, 6000, 4),
    ("echo", 6000, 7200, 4),
    ("pop", 7480, 7720, 5),
];

const MIXED_PIECES: &[Piece] = &[
    ("长音符", 300, 1800, 0),
    ("short", 2050, 2350, 1),
    ("长旋律", 2700, 4300, 2),
    ("tap", 4550, 4800, 3),
    ("long", 5200, 6000, 4),
    ("echo", 6000, 7200, 4),
    ("快", 7480, 7720, 5),
];

fn timings(line: &'static str, pieces: &'static [Piece]) -> Vec<TimedTextUnit> {
    // Units can be graphemes, words, or arbitrary UTF-8 ranges. Characters in
    // the same group reveal independently but lift/scale as one phrase.
    let mut search_from = 0;
    pieces
        .iter()
        .copied()
        .map(|(piece, start, end, group)| {
            let relative = line[search_from..]
                .find(piece)
                .expect("timed fragment must occur in order");
            let range_start = search_from + relative;
            let range_end = range_start + piece.len();
            search_from = range_end;
            TimedTextUnit::new(
                range_start..range_end,
                Duration::from_millis(start),
                Duration::from_millis(end),
            )
            .group(group)
        })
        .collect()
}

fn timed_row(
    label: &'static str,
    animation_id: &'static str,
    line: &'static str,
    pieces: &'static [Piece],
    fill: Background,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_4()
        .child(
            div()
                .w(px(54.))
                .text_xs()
                .text_color(rgba(0xe7edf870))
                .child(label),
        )
        .child(
            TimedText::new(line, timings(line, pieces))
                .active_fill(fill)
                .inactive_opacity(0.25)
                // Only these semantic groups receive elastic emphasis.
                .elastic_groups([0, 2, 4])
                .emphasis(TimedTextEmphasis {
                    scale: 1.12,
                    translation: point(px(0.), px(-5.)),
                    surrounding_spread: px(7.),
                    ..Default::default()
                })
                .text_color(rgb(0xe7edf8))
                .text_size(px(28.))
                .font_weight(FontWeight::SEMIBOLD)
                .with_animation(
                    animation_id,
                    Animation::new(Duration::from_secs_f32(LOOP_SECONDS)).repeat(),
                    |line, phase| line.position(Duration::from_secs_f32(phase * LOOP_SECONDS)),
                ),
        )
}

struct TimedTextExample;

impl Render for TimedTextExample {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let fill = multi_linear_gradient(
            90.0,
            [
                linear_color_stop(rgb(0x67e8f9), 0.0),
                linear_color_stop(rgb(0xa78bfa), 0.45),
                linear_color_stop(rgb(0xf9a8d4), 1.0),
            ],
        );

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x090d18))
            .child(
                GlassPanel::new()
                    .material(GlassMaterial::Regular)
                    .radius(px(28.))
                    .tint(rgba(0x11182a80))
                    .glass_opacity(0.76)
                    .w(px(900.))
                    .px(px(42.))
                    .py(px(48.))
                    .flex()
                    .flex_col()
                    .gap_5()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .text_sm()
                            .child(
                                div()
                                    .text_color(rgb(0x8be9fd))
                                    .child("ELASTIC GROUPS  ·  0 / 2 / 4"),
                            )
                            .child(
                                div()
                                    .text_color(rgba(0xe7edf880))
                                    .child("STEADY GROUPS   ·  1 / 3 / 5"),
                            )
                            .child(
                                div()
                                    .text_color(rgba(0xe7edf860))
                                    .child("SEGMENTATION  ·  EXPLICIT UTF-8 RANGES"),
                            ),
                    )
                    .child(timed_row(
                        "中文",
                        "timed-text-chinese",
                        CHINESE_LINE,
                        CHINESE_PIECES,
                        fill,
                    ))
                    .child(timed_row(
                        "EN",
                        "timed-text-english",
                        ENGLISH_LINE,
                        ENGLISH_PIECES,
                        fill,
                    ))
                    .child(timed_row(
                        "混合",
                        "timed-text-mixed",
                        MIXED_LINE,
                        MIXED_PIECES,
                        fill,
                    )),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1280.), px(620.)),
                    cx,
                ))),
                ..Default::default()
            },
            |_, cx| cx.new(|_| TimedTextExample),
        )
        .expect("failed to open timed text example");
    });
}
