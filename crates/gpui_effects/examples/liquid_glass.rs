use std::{
    f32::consts::TAU,
    time::{Duration, Instant},
};

use gpui::{
    Animation, AnimationExt, App, Bounds, Context, CursorStyle, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, Window, WindowBounds, WindowOptions, div,
    point, prelude::*, px, rgb, rgba, size,
};
use gpui_effects::{GlassMaterial, GlassPanel};
use gpui_platform::application;

const PROBE_WIDTH: f32 = 126.0;
const PROBE_HEIGHT: f32 = 92.0;

struct LiquidGlassExample {
    probe_position: Point<Pixels>,
    probe_drag_offset: Point<Pixels>,
    probe_velocity: Point<f32>,
    last_probe_move_at: Instant,
    last_frame_at: Instant,
    dragging_probe: bool,
}

impl LiquidGlassExample {
    fn begin_probe_drag(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.dragging_probe = true;
        self.probe_drag_offset = point(
            event.position.x - self.probe_position.x,
            event.position.y - self.probe_position.y,
        );
        self.probe_velocity = Point::default();
        self.last_probe_move_at = Instant::now();
        cx.notify();
    }

    fn update_probe_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.dragging_probe {
            return;
        }

        let viewport = window.viewport_size();
        let max_x = (viewport.width.as_f32() - PROBE_WIDTH).max(0.0);
        let max_y = (viewport.height.as_f32() - PROBE_HEIGHT).max(0.0);
        let next_position = point(
            px((event.position.x - self.probe_drag_offset.x)
                .as_f32()
                .clamp(0.0, max_x)),
            px((event.position.y - self.probe_drag_offset.y)
                .as_f32()
                .clamp(0.0, max_y)),
        );
        let now = Instant::now();
        let elapsed = now
            .saturating_duration_since(self.last_probe_move_at)
            .as_secs_f32();
        if (0.001..=0.15).contains(&elapsed) {
            let measured = Point {
                x: (next_position.x - self.probe_position.x).as_f32() / elapsed,
                y: (next_position.y - self.probe_position.y).as_f32() / elapsed,
            };
            let blend = 1.0 - (-elapsed * 24.0).exp();
            self.probe_velocity.x += (measured.x - self.probe_velocity.x) * blend;
            self.probe_velocity.y += (measured.y - self.probe_velocity.y) * blend;
        }
        self.probe_position = next_position;
        self.last_probe_move_at = now;
        cx.notify();
    }

    fn end_probe_drag(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.dragging_probe = false;
        cx.notify();
    }
}

impl Render for LiquidGlassExample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let support = if window.supports_backdrop_blur() {
            "LIVE BACKDROP / REFRACTION ENABLED"
        } else {
            "TRANSLUCENT FALLBACK"
        };
        let viewport = window.viewport_size();
        self.probe_position.x = px(self
            .probe_position
            .x
            .as_f32()
            .clamp(0.0, (viewport.width.as_f32() - PROBE_WIDTH).max(0.0)));
        self.probe_position.y = px(self
            .probe_position
            .y
            .as_f32()
            .clamp(0.0, (viewport.height.as_f32() - PROBE_HEIGHT).max(0.0)));
        let now = Instant::now();
        let frame_elapsed = now
            .saturating_duration_since(self.last_frame_at)
            .as_secs_f32()
            .clamp(0.0, 1.0 / 20.0);
        self.last_frame_at = now;
        let velocity_decay = (-frame_elapsed * if self.dragging_probe { 2.0 } else { 5.0 }).exp();
        self.probe_velocity.x *= velocity_decay;
        self.probe_velocity.y *= velocity_decay;
        if self.probe_velocity.x.abs() + self.probe_velocity.y.abs() < 0.5 {
            self.probe_velocity = Point::default();
        }

        // Paint the probe last so it remains draggable while crossing the
        // color band or the larger glass panel.
        let probe = div()
            .absolute()
            .left(self.probe_position.x)
            .top(self.probe_position.y)
            .w(px(PROBE_WIDTH))
            .h(px(PROBE_HEIGHT))
            .cursor(if self.dragging_probe {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .child(
                GlassPanel::new()
                    .material(GlassMaterial::Thin)
                    .animation("glass-drag-probe", Duration::from_secs(6))
                    .translation_velocity(self.probe_velocity)
                    .radius(px(42.0))
                    .size_full()
                    .shadow_xl()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .child(
                        div()
                            .size(px(9.0))
                            .rounded_full()
                            .bg(rgba(0xffffffd8))
                            .shadow_sm(),
                    )
                    .child(div().text_xs().text_color(rgba(0xffffffb8)).child("DRAG")),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(Self::begin_probe_drag))
            .on_mouse_move(cx.listener(Self::update_probe_drag))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::end_probe_drag))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::end_probe_drag));

        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(rgb(0x080d18))
            .text_color(rgb(0xffffff))
            // A dense optical grid makes even small refraction visible.
            .children((0..26).map(|column| {
                div()
                    .absolute()
                    .left(px(column as f32 * 40.0))
                    .top_0()
                    .bottom_0()
                    .w(px(if column % 5 == 0 { 2.0 } else { 1.0 }))
                    .bg(rgba(if column % 5 == 0 {
                        0x44d7ff3d
                    } else {
                        0x7aa5ca20
                    }))
            }))
            .children((0..17).map(|row| {
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(row as f32 * 40.0))
                    .h(px(if row % 5 == 0 { 2.0 } else { 1.0 }))
                    .bg(rgba(if row % 5 == 0 {
                        0x44d7ff3d
                    } else {
                        0x7aa5ca20
                    }))
            }))
            .child(
                div()
                    .absolute()
                    .left(px(42.0))
                    .top(px(30.0))
                    .text_xs()
                    .text_color(rgba(0x8edfffc0))
                    .child("GPUI EFFECTS  /  OPTICAL MATERIAL LAB"),
            )
            .child(
                div()
                    .absolute()
                    .left(px(84.0))
                    .top(px(238.0))
                    .text_2xl()
                    .text_color(rgba(0xffffff78))
                    .child("REFRACTION  0123456789  REFRACTION"),
            )
            .child(
                div()
                    .absolute()
                    .left(px(110.0))
                    .top(px(354.0))
                    .text_2xl()
                    .text_color(rgba(0xffffff68))
                    .child("STRAIGHT LINES SHOULD BEND AT THE EDGE"),
            )
            // Fine black/white bars expose magnification and chromatic separation.
            .child(
                div()
                    .absolute()
                    .left(px(174.0))
                    .top(px(300.0))
                    .w(px(650.0))
                    .h(px(36.0))
                    .overflow_hidden()
                    .flex()
                    .children((0..65).map(|index| {
                        div()
                            .h_full()
                            .w(px(10.0))
                            .bg(rgb(if index % 2 == 0 { 0xf2f7ff } else { 0x172033 }))
                    })),
            )
            // A luminous bead repeatedly crosses both glass edges.
            .child(
                div()
                    .absolute()
                    .left(px(80.0))
                    .top(px(270.0))
                    .size(px(74.0))
                    .rounded_full()
                    .bg(rgb(0xff4f9a))
                    .border_4()
                    .border_color(rgba(0xffd4e8d8))
                    .shadow_xl()
                    .with_animation(
                        "glass-reference-bead",
                        Animation::new(Duration::from_secs(12)).repeat(),
                        |bead, time| {
                            let phase = time * TAU;
                            bead.left(px(455.0 + phase.sin() * 360.0))
                                .top(px(270.0 + (phase * 2.0).sin() * 92.0))
                        },
                    ),
            )
            // This color band makes background color transmission obvious.
            .child(
                div()
                    .absolute()
                    .left(px(105.0))
                    .top(px(430.0))
                    .w(px(790.0))
                    .h(px(68.0))
                    .flex()
                    .children([
                        div().flex_1().h_full().bg(rgb(0x6c4dff)),
                        div().flex_1().h_full().bg(rgb(0x00d4ba)),
                        div().flex_1().h_full().bg(rgb(0xffc857)),
                        div().flex_1().h_full().bg(rgb(0xff4f87)),
                    ]),
            )
            .child(
                GlassPanel::new()
                    .material(GlassMaterial::Regular)
                    .animation("glass-example", Duration::from_secs(7))
                    .absolute()
                    .left(px(225.0))
                    .top(px(155.0))
                    .w(px(550.0))
                    .h(px(310.0))
                    .p_7()
                    .shadow_xl()
                    .flex()
                    .flex_col()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_2xl().child("Liquid Glass"))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgba(0xffffffc4))
                                            .child("Move, press and release anywhere on the surface"),
                                    ),
                            )
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .rounded_full()
                                    .bg(rgba(0xffffff18))
                                    .text_xs()
                                    .child("INTERACTIVE"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_xs()
                            .text_color(rgba(0xffffffaa))
                            .child(support)
                            .child("PRESSURE  /  INERTIA  /  SPRING"),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(42.0))
                    .bottom(px(28.0))
                    .text_xs()
                    .text_color(rgba(0xffffff70))
                    .child("Watch grid spacing, stripe width and the moving bead at the glass boundary."),
            )
            .child(probe)
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1000.0), px(640.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                let now = Instant::now();
                cx.new(|_| LiquidGlassExample {
                    probe_position: point(px(690.0), px(80.0)),
                    probe_drag_offset: Point::default(),
                    probe_velocity: Point::default(),
                    last_probe_move_at: now,
                    last_frame_at: now,
                    dragging_probe: false,
                })
            },
        )
        .expect("failed to open liquid glass example");
        cx.activate(true);
    });
}
