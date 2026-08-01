use gpui::{Context, IntoElement, Render, Window, WindowOptions, div, prelude::*, px, rgb};
use uic::components::modal::{self, Modal, ModalAppearance, ModalPlacement};

struct ModalExample;

impl Render for ModalExample {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_layer = modal::layer(cx);

        div()
            .relative()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .gap_3()
            .bg(rgb(0xf8fafc))
            .child(
                div()
                    .id("show-centered-modal")
                    .px_4()
                    .py_2()
                    .rounded_lg()
                    .bg(rgb(0x2563eb))
                    .text_color(rgb(0xffffff))
                    .cursor_pointer()
                    .on_click(|_, window, cx| {
                        modal::show(
                            Modal::new(|_, _| {
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_lg().child("Custom modal content"))
                                    .child("Simple content does not need an Entity.")
                            })
                            .title(|_, _| div().flex().gap_2().child("◆").child("Centered modal"))
                            .close_button(|_, _| {
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .hover(|s| s.bg(rgb(0xe2e8f0)))
                                    .child("×")
                            })
                            .ok_text(|_, _| div().flex().gap_2().child("✓").child("Save"))
                            .cancel_label("Back")
                            .appearance(ModalAppearance::default().border(false))
                            .on_ok(|_, _| true),
                            window,
                            cx,
                        );
                    })
                    .child("Centered"),
            )
            .child(
                div()
                    .id("show-top-modal")
                    .px_4()
                    .py_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(0xcbd5e1))
                    .cursor_pointer()
                    .on_click(|_, window, cx| {
                        modal::show(
                            Modal::new(|_, _| div().child("Top modal content"))
                                .title_text("Top modal")
                                .placement(ModalPlacement::Top { offset: px(64.) }),
                            window,
                            cx,
                        );
                    })
                    .child("Top"),
            )
            // The layer must be the last child so it is above application content.
            .child(modal_layer)
    }
}

fn main() {
    gpui_platform::application().run(|cx| {
        modal::init_with_appearance(
            ModalAppearance {
                radius: px(16.),
                ..ModalAppearance::default()
            },
            cx,
        );

        cx.open_window(WindowOptions::default(), |_, cx| cx.new(|_| ModalExample))
            .expect("failed to open modal example window");
    });
}
