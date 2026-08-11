use std::time::Instant;

use gpui::{
    App, Bounds, Context, CursorStyle, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Render, Window, WindowBounds, WindowOptions, div,
    linear_color_stop, linear_gradient, point, prelude::*, px, rgb, rgba, size,
};
use gpui_effects::{GlassMaterial, GlassPanel};
use gpui_platform::application;

const GLASS_WIDTH: Pixels = px(300.0);
const GLASS_HEIGHT: Pixels = px(200.0);
const GLASS_RADIUS: Pixels = px(48.0);
const VIEWPORT_MARGIN: Pixels = px(12.0);

struct LiquidGlassDemo {
    position: Point<Pixels>,
    drag_offset: Point<Pixels>,
    dragging: bool,
    last_pointer: Option<(Point<Pixels>, Instant)>,
    velocity: Point<f32>,
}

impl LiquidGlassDemo {
    fn new() -> Self {
        Self {
            position: point(px(400.0), px(230.0)),
            drag_offset: Point::default(),
            dragging: false,
            last_pointer: None,
            velocity: Point::default(),
        }
    }

    fn constrain_position(position: Point<Pixels>, window: &Window) -> Point<Pixels> {
        let viewport = window.viewport_size();
        let max_x = (viewport.width - GLASS_WIDTH - VIEWPORT_MARGIN).max(VIEWPORT_MARGIN);
        let max_y = (viewport.height - GLASS_HEIGHT - VIEWPORT_MARGIN).max(VIEWPORT_MARGIN);
        point(
            position.x.clamp(VIEWPORT_MARGIN, max_x),
            position.y.clamp(VIEWPORT_MARGIN, max_y),
        )
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dragging = true;
        self.drag_offset = event.position - self.position;
        self.last_pointer = Some((event.position, Instant::now()));
        self.velocity = Point::default();
        cx.notify();
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.dragging {
            return;
        }
        if !event.dragging() {
            self.dragging = false;
            self.last_pointer = None;
            self.velocity = Point::default();
            cx.notify();
            return;
        }

        let now = Instant::now();
        let next = Self::constrain_position(event.position - self.drag_offset, window);
        if let Some((previous, previous_at)) = self.last_pointer {
            let elapsed = now
                .saturating_duration_since(previous_at)
                .as_secs_f32()
                .max(1.0 / 1000.0);
            self.velocity = Point {
                x: (event.position.x - previous.x).as_f32() / elapsed,
                y: (event.position.y - previous.y).as_f32() / elapsed,
            };
        }
        self.position = next;
        self.last_pointer = Some((event.position, now));
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.dragging {
            self.dragging = false;
            self.last_pointer = None;
            self.velocity = Point::default();
            cx.notify();
        }
    }

    fn refraction_cards() -> impl IntoElement {
        let card = || {
            div()
                .relative()
                .w(px(178.0))
                .h(px(150.0))
                .p(px(18.0))
                .overflow_hidden()
                .rounded(px(24.0))
                .border_1()
                .border_color(rgba(0xe6f6ff38))
                .bg(rgba(0x08122a8c))
                .shadow_lg()
                .flex()
                .flex_col()
                .justify_between()
        };

        div()
            .absolute()
            .right(px(54.0))
            .bottom(px(54.0))
            .flex()
            .gap(px(14.0))
            .child(
                card()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("TYPOGRAPHY"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .line_height(px(20.0))
                            .text_color(rgba(0xebf6ff8f))
                            .child("ABCDEFG")
                            .child("0123456789"),
                    ),
            )
            .child(
                card()
                    .children((0..12).map(|stripe| {
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(stripe as f32 * 18.0 - 8.0))
                            .w(px(8.0))
                            .bg(rgba(0xffffff24))
                    }))
                    .child(
                        div()
                            .relative()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("FINE DETAIL"),
                    )
                    .child(
                        div()
                            .relative()
                            .text_xs()
                            .line_height(px(20.0))
                            .text_color(rgba(0xebf6ff9f))
                            .child("8 PX STRIPES")
                            .child("EDGE TEST"),
                    ),
            )
            .child(
                card()
                    .bg(linear_gradient(
                        135.0,
                        linear_color_stop(rgba(0xff4e85b8), 0.0),
                        linear_color_stop(rgba(0x00cfffcc), 1.0),
                    ))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("SPECTRUM"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .line_height(px(20.0))
                            .text_color(rgba(0xf5fbffbd))
                            .child("RED · VIOLET")
                            .child("CYAN"),
                    ),
            )
    }

    fn glass(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let position = Self::constrain_position(self.position, window);
        GlassPanel::liquid()
            .id("draggable-liquid-glass")
            .material(GlassMaterial::Regular)
            .radius(GLASS_RADIUS)
            .tint(rgba(0xd1efff0e))
            .edge_color(rgba(0xe1f5ffb8))
            .absolute()
            .left(position.x)
            .top(position.y)
            .w(GLASS_WIDTH)
            .h(GLASS_HEIGHT)
            .overflow_hidden()
            .border_1()
            .border_color(rgba(0xf0faff6b))
            .shadow_2xl()
            .cursor(if self.dragging {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_color(rgba(0xf8fcfff0))
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Drag me"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgba(0xf8fcffa3))
                            .child("LIQUID GLASS"),
                    ),
            )
    }
}

impl Render for LiquidGlassDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(linear_gradient(
                145.0,
                linear_color_stop(rgb(0x101a3d), 0.0),
                linear_color_stop(rgb(0x0a1025), 1.0),
            ))
            .text_color(rgb(0xf7fbff))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .children((0..18).map(|column| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(column as f32 * 72.0))
                    .w(px(1.0))
                    .bg(rgba(0xd9efff20))
            }))
            .children((0..12).map(|row| {
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(row as f32 * 72.0))
                    .h(px(1.0))
                    .bg(rgba(0xd9efff20))
            }))
            .child(
                div()
                    .absolute()
                    .left(px(-70.0))
                    .top(px(-60.0))
                    .size(px(360.0))
                    .rounded_full()
                    .bg(rgba(0xff4680b8)),
            )
            .child(
                div()
                    .absolute()
                    .right(px(-40.0))
                    .top(px(6.0))
                    .size(px(390.0))
                    .rounded_full()
                    .bg(rgba(0x00c6ffb5)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(390.0))
                    .bottom(px(-250.0))
                    .size(px(560.0))
                    .rounded_full()
                    .bg(rgba(0x8953ffad)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(58.0))
                    .top(px(44.0))
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgba(0xc3ebffdb))
                    .child("WGSL BACKDROP DISPLACEMENT"),
            )
            .child(
                div()
                    .absolute()
                    .left(px(52.0))
                    .top(px(82.0))
                    .flex()
                    .flex_col()
                    .text_size(px(82.0))
                    .line_height(px(70.0))
                    .font_weight(FontWeight::BOLD)
                    .child("Liquid")
                    .child("Glass"),
            )
            .child(
                div()
                    .absolute()
                    .left(px(58.0))
                    .top(px(246.0))
                    .w(px(420.0))
                    .text_lg()
                    .line_height(px(30.0))
                    .text_color(rgba(0xebf5ffa8))
                    .child(
                        "Drag the glass across text, grid lines, color fields, and fine stripes to inspect refraction and magnification.",
                    ),
            )
            .child(Self::refraction_cards())
            .child(self.glass(window, cx))
            .child(
                div()
                    .absolute()
                    .left(px(24.0))
                    .bottom(px(20.0))
                    .px_3()
                    .py_2()
                    .rounded_full()
                    .border_1()
                    .border_color(rgba(0xffffff29))
                    .bg(rgba(0x050c1e7a))
                    .text_xs()
                    .text_color(rgba(0xf5fbffb8))
                    .child(if self.dragging {
                        format!(
                            "DRAGGING | {:.0}, {:.0} PX/S",
                            self.velocity.x, self.velocity.y
                        )
                    } else {
                        "PRESS AND DRAG | MOVE THE GLASS".to_owned()
                    }),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1180.0), px(760.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |_, cx| cx.new(|_| LiquidGlassDemo::new()),
        )
        .expect("failed to open liquid glass example");
        cx.activate(true);
    });
}
