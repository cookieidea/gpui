use std::time::Duration;

use gpui::{
    App, Context, Entity, Global, Hsla, MouseButton, Pixels, Render, SharedString, Window, div,
    prelude::*, px, rgb,
};

const DEFAULT_DURATION: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastVariant {
    Success,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastPlacement {
    #[default]
    TopRight,
    BottomRight,
}

#[derive(Clone, Copy, Debug)]
pub struct ToastColors {
    pub background: Hsla,
    pub foreground: Hsla,
    pub border: Hsla,
    pub accent: Hsla,
    pub accent_background: Hsla,
}

#[derive(Clone, Copy, Debug)]
pub struct ToastAppearance {
    pub success: ToastColors,
    pub warn: ToastColors,
    pub error: ToastColors,
    pub offset_x: Pixels,
    pub offset_y: Pixels,
    pub gap: Pixels,
    pub min_width: Pixels,
    pub max_width: Pixels,
    pub radius: Pixels,
}

impl ToastAppearance {
    fn colors(&self, variant: ToastVariant) -> ToastColors {
        match variant {
            ToastVariant::Success => self.success,
            ToastVariant::Warn => self.warn,
            ToastVariant::Error => self.error,
        }
    }
}

impl Default for ToastAppearance {
    fn default() -> Self {
        Self {
            success: ToastColors {
                background: rgb(0x052e16).into(),
                foreground: rgb(0xdcfce7).into(),
                border: rgb(0x166534).into(),
                accent: rgb(0x22c55e).into(),
                accent_background: rgb(0x14532d).into(),
            },
            warn: ToastColors {
                background: rgb(0x451a03).into(),
                foreground: rgb(0xfef3c7).into(),
                border: rgb(0x92400e).into(),
                accent: rgb(0xf59e0b).into(),
                accent_background: rgb(0x78350f).into(),
            },
            error: ToastColors {
                background: rgb(0x450a0a).into(),
                foreground: rgb(0xfee2e2).into(),
                border: rgb(0x991b1b).into(),
                accent: rgb(0xef4444).into(),
                accent_background: rgb(0x7f1d1d).into(),
            },
            offset_x: px(20.),
            offset_y: px(20.),
            gap: px(10.),
            min_width: px(300.),
            max_width: px(440.),
            radius: px(12.),
        }
    }
}

#[derive(Clone)]
struct ToastItem {
    id: u64,
    message: SharedString,
    variant: ToastVariant,
    placement: ToastPlacement,
}

pub struct ToastManager {
    items: Vec<ToastItem>,
    next_id: u64,
    appearance: ToastAppearance,
    placement: ToastPlacement,
}

impl ToastManager {
    fn new(appearance: ToastAppearance) -> Self {
        Self {
            items: Vec::new(),
            next_id: 1,
            appearance,
            placement: ToastPlacement::default(),
        }
    }

    fn push(
        &mut self,
        message: impl Into<SharedString>,
        variant: ToastVariant,
        duration: Duration,
        placement: Option<ToastPlacement>,
        cx: &mut Context<Self>,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(ToastItem {
            id,
            message: message.into(),
            variant,
            placement: placement.unwrap_or(self.placement),
        });
        cx.notify();

        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |manager, cx| manager.dismiss(id, cx));
            }
        })
        .detach();
    }

    fn dismiss(&mut self, id: u64, cx: &mut Context<Self>) {
        self.items.retain(|item| item.id != id);
        cx.notify();
    }
}

impl Render for ToastManager {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().absolute().size_full().children([
            self.render_stack(ToastPlacement::TopRight, cx),
            self.render_stack(ToastPlacement::BottomRight, cx),
        ])
    }
}

impl ToastManager {
    fn render_stack(&self, placement: ToastPlacement, cx: &mut Context<Self>) -> gpui::Div {
        let appearance = self.appearance;
        div()
            .absolute()
            .right(appearance.offset_x)
            .when_else(
                placement == ToastPlacement::TopRight,
                |this| this.top(appearance.offset_y).flex_col(),
                |this| this.bottom(appearance.offset_y).flex_col_reverse(),
            )
            .flex()
            .items_end()
            .gap(appearance.gap)
            .children(
                self.items
                    .iter()
                    .filter(move |item| item.placement == placement)
                    .map(|item| {
                        let id = item.id;
                        let colors = appearance.colors(item.variant);
                        div()
                            .id(("toast", id as usize))
                            .flex()
                            .items_center()
                            .gap_3()
                            .min_w(appearance.min_width)
                            .max_w(appearance.max_width)
                            .px_3()
                            .py_3()
                            .rounded(appearance.radius)
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.background)
                            .shadow_lg()
                            .cursor_pointer()
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(move |manager, _, _, cx| manager.dismiss(id, cx)),
                            )
                            .child(
                                div()
                                    .size_8()
                                    .flex_none()
                                    .rounded_full()
                                    .bg(colors.accent_background)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(div().size_2().rounded_full().bg(colors.accent)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .text_color(colors.foreground)
                                    .child(item.message.clone()),
                            )
                    }),
            )
    }
}

struct GlobalToast(Entity<ToastManager>);

impl Global for GlobalToast {}

pub fn init(cx: &mut App) {
    init_with_appearance(ToastAppearance::default(), cx);
}

pub fn init_with_appearance(appearance: ToastAppearance, cx: &mut App) {
    if !cx.has_global::<GlobalToast>() {
        let manager = cx.new(|_| ToastManager::new(appearance));
        cx.set_global(GlobalToast(manager));
    } else {
        set_appearance(appearance, cx);
    }
}

pub fn set_appearance(appearance: ToastAppearance, cx: &mut App) {
    let manager = layer(cx);
    manager.update(cx, |manager, cx| {
        manager.appearance = appearance;
        cx.notify();
    });
}

pub fn set_placement(placement: ToastPlacement, cx: &mut App) {
    layer(cx).update(cx, |manager, cx| {
        manager.placement = placement;
        cx.notify();
    });
}

pub fn layer(cx: &App) -> Entity<ToastManager> {
    cx.global::<GlobalToast>().0.clone()
}

pub fn success(message: impl Into<SharedString>, cx: &mut App) {
    show(message, ToastVariant::Success, DEFAULT_DURATION, cx);
}

pub fn success_at(message: impl Into<SharedString>, placement: ToastPlacement, cx: &mut App) {
    show_at(
        message,
        ToastVariant::Success,
        DEFAULT_DURATION,
        placement,
        cx,
    );
}

pub fn warn(message: impl Into<SharedString>, cx: &mut App) {
    show(message, ToastVariant::Warn, DEFAULT_DURATION, cx);
}

pub fn warn_at(message: impl Into<SharedString>, placement: ToastPlacement, cx: &mut App) {
    show_at(message, ToastVariant::Warn, DEFAULT_DURATION, placement, cx);
}

pub fn error(message: impl Into<SharedString>, cx: &mut App) {
    show(message, ToastVariant::Error, DEFAULT_DURATION, cx);
}

pub fn error_at(message: impl Into<SharedString>, placement: ToastPlacement, cx: &mut App) {
    show_at(
        message,
        ToastVariant::Error,
        DEFAULT_DURATION,
        placement,
        cx,
    );
}

pub fn show(
    message: impl Into<SharedString>,
    variant: ToastVariant,
    duration: Duration,
    cx: &mut App,
) {
    let manager = layer(cx);
    manager.update(cx, |manager, cx| {
        manager.push(message, variant, duration, None, cx);
    });
}

pub fn show_at(
    message: impl Into<SharedString>,
    variant: ToastVariant,
    duration: Duration,
    placement: ToastPlacement,
    cx: &mut App,
) {
    layer(cx).update(cx, |manager, cx| {
        manager.push(message, variant, duration, Some(placement), cx);
    });
}
