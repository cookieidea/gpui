use gpui::{
    App, Bounds, Context, Render, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb,
    rgba, size,
};
use gpui_platform::application;

struct BackdropBlurExample;

impl Render for BackdropBlurExample {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let support = if window.supports_backdrop_blur() {
            "WGPU backdrop blur"
        } else {
            "translucent fallback"
        };

        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(rgb(0x111827))
            .text_color(rgb(0xffffff))
            .child(
                div()
                    .absolute()
                    .left(px(70.))
                    .top(px(55.))
                    .size(px(260.))
                    .rounded_full()
                    .bg(rgb(0x7c3aed)),
            )
            .child(
                div()
                    .absolute()
                    .right(px(65.))
                    .bottom(px(35.))
                    .size(px(300.))
                    .rounded_full()
                    .bg(rgb(0x06b6d4)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(210.))
                    .top(px(175.))
                    .w(px(330.))
                    .h(px(190.))
                    .p_6()
                    .rounded_3xl()
                    .backdrop_blur(px(28.))
                    .bg(rgba(0xffffff26))
                    .border_1()
                    .border_color(rgba(0xffffff59))
                    .shadow_xl()
                    .flex()
                    .flex_col()
                    .justify_between()
                    .child(div().text_2xl().child("Backdrop Blur"))
                    .child(div().text_sm().text_color(rgba(0xffffffcc)).child(support)),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(760.), px(480.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| BackdropBlurExample),
        )
        .unwrap();
        cx.activate(true);
    });
}
