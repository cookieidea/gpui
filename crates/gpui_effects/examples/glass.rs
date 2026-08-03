use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, Bounds, Context, FontWeight, Render, Rgba, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, rgba, size,
};
use gpui_effects::{GlassMaterial, GlassPanel};
use gpui_platform::application;

fn panel_content(
    eyebrow: &'static str,
    title: &'static str,
    description: &'static str,
    accent: Rgba,
) -> impl IntoElement {
    div()
        .size_full()
        .flex()
        .flex_col()
        .justify_between()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_xs().text_color(accent).child(eyebrow))
                .child(
                    div()
                        .px_3()
                        .py_1()
                        .rounded_full()
                        .border_1()
                        .border_color(accent.opacity(0.38))
                        .text_xs()
                        .text_color(rgba(0xeaf8ffb8))
                        .child("BACKDROP"),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_3xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0xf4fbff))
                        .child(title),
                )
                .child(
                    div()
                        .max_w(px(244.0))
                        .text_sm()
                        .line_height(px(22.0))
                        .text_color(rgba(0xe4f3ff9c))
                        .child(description),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_xs()
                .text_color(rgba(0xddefff78))
                .child(div().size(px(6.0)).rounded_full().bg(accent))
                .child(if eyebrow == "FROSTED" {
                    "STABLE / DIFFUSE"
                } else {
                    "MOVE POINTER / PRESS"
                }),
        )
}

fn glass_scene(phase: f32) -> impl IntoElement {
    let tau = std::f32::consts::TAU;
    let horizontal = (phase * tau).sin();
    let vertical = (phase * tau * 2.0).cos();

    div()
        .relative()
        .w(px(720.0))
        .h(px(400.0))
        .overflow_hidden()
        .rounded(px(36.0))
        .border_1()
        .border_color(rgba(0xb7eaff24))
        .bg(rgb(0x07111e))
        // Everything below is painted first and becomes the live backdrop.
        .children((0..12).map(|column| {
            div()
                .absolute()
                .left(px(24.0 + column as f32 * 60.0))
                .top_0()
                .bottom_0()
                .w(px(1.0))
                .bg(rgba(0x6bdcff18))
        }))
        .children((0..7).map(|row| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(34.0 + row as f32 * 55.0))
                .h(px(1.0))
                .bg(rgba(0x8b8dff18))
        }))
        .child(
            div()
                .absolute()
                .left(px(58.0 + horizontal * 34.0))
                .top(px(32.0 + vertical * 16.0))
                .size(px(190.0))
                .rounded_full()
                .bg(rgba(0x00c8ff66)),
        )
        .child(
            div()
                .absolute()
                .right(px(28.0 + horizontal * 42.0))
                .bottom(px(8.0 + vertical * 14.0))
                .size(px(228.0))
                .rounded_full()
                .bg(rgba(0x7357ff70)),
        )
        .child(
            div()
                .absolute()
                .left(px(254.0 - horizontal * 44.0))
                .bottom(px(40.0 + vertical * 10.0))
                .w(px(250.0))
                .h(px(60.0))
                .rounded_full()
                .bg(rgba(0xff4f9a78)),
        )
        .child(
            div()
                .absolute()
                .left(px(34.0))
                .top(px(174.0))
                .text_size(px(32.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(0xffffff5c))
                .child("REFRACTION 0123456789"),
        )
        // Glass panels are painted after the scene. Their children stay crisp.
        .child(
            GlassPanel::frosted()
                .id("frosted-example")
                .material(GlassMaterial::Regular)
                .radius(px(30.0))
                .tint(rgba(0xc9efff24))
                .edge_color(rgba(0xe7fbffb8))
                .absolute()
                .left(px(28.0))
                .top(px(58.0))
                .w(px(310.0))
                .h(px(284.0))
                .p(px(22.0))
                .child(panel_content(
                    "FROSTED",
                    "Diffuse glass",
                    "Stable scattering softens the moving scene without bending the panel content.",
                    rgba(0x8de8ffdc),
                )),
        )
        .child(
            GlassPanel::gel()
                .id("gel-example")
                .material(GlassMaterial::Regular)
                .radius(px(30.0))
                .tint(rgba(0xc8d2ff18))
                .edge_color(rgba(0xf1f4ffcc))
                .deformation(0.92)
                .wave_strength(0.72)
                .animation("gel-example-loop", Duration::from_secs(8))
                .absolute()
                .right(px(28.0))
                .top(px(58.0))
                .w(px(310.0))
                .h(px(284.0))
                .p(px(22.0))
                .child(panel_content(
                    "GEL",
                    "Elastic glass",
                    "Continuous optical flow, pointer response, and a damped release keep this surface alive.",
                    rgba(0xb9b7ffdc),
                )),
        )
}

struct GlassExample;

impl Render for GlassExample {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(32.0))
            .bg(rgb(0x030914))
            .with_animation(
                "glass-example-backdrop",
                Animation::new(Duration::from_secs(12)).repeat(),
                |root, phase| root.child(glass_scene(phase)),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.0), px(720.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |_, cx| cx.new(|_| GlassExample),
        )
        .expect("failed to open glass example");
    });
}
