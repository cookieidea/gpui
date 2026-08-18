use gpui::{
    AnyElement, App, CursorStyle, Entity, IntoElement, MouseButton, Refineable as _, RenderOnce,
    StyleRefinement, Styled, Window, div, prelude::*, px,
};

use super::{InputAppearance, TextInput};

#[derive(IntoElement)]
pub struct Input {
    state: Entity<TextInput>,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    appearance: InputAppearance,
    rows: Option<usize>,
    style: StyleRefinement,
}

input_appearance!(Input);

impl Input {
    pub fn new(state: &Entity<TextInput>) -> Self {
        Self {
            state: state.clone(),
            prefix: None,
            suffix: None,
            appearance: InputAppearance::default(),
            rows: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn prefix(mut self, prefix: impl IntoElement) -> Self {
        self.prefix = Some(prefix.into_any_element());
        self
    }

    pub fn suffix(mut self, suffix: impl IntoElement) -> Self {
        self.suffix = Some(suffix.into_any_element());
        self
    }

    pub fn appearance(mut self, appearance: InputAppearance) -> Self {
        self.appearance = appearance;
        self
    }
}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let appearance = self.appearance;
        self.state
            .update(cx, |state, _cx| state.appearance = appearance);

        let (focused, focus_handle, disabled, multiline) = {
            let state = self.state.read(cx);
            (
                state.focus_handle.is_focused(window),
                state.focus_handle.clone(),
                state.disabled,
                state.mode == super::InputMode::Multiline,
            )
        };

        let row_height = self
            .rows
            .map(|rows| super::row_height(&self.style, rows, window.rem_size()));

        let mut element = div()
            .flex()
            .when(multiline, |this| this.items_start())
            .when(!multiline, |this| this.items_center())
            .w_full()
            .h(px(44.))
            .when_some(row_height.filter(|_| multiline), |this, height| {
                this.h(height)
            })
            .px(px(14.))
            .when(multiline, |this| this.py(px(10.)))
            .gap(px(10.))
            .text_size(px(16.))
            .line_height(px(24.))
            .text_color(gpui::hsla(0., 0., 0.08, 1.))
            .rounded(px(10.))
            .border(px(1.))
            .border_color(if focused && !disabled {
                appearance.focus_border
            } else {
                gpui::hsla(0., 0., 0.75, 1.)
            })
            .bg(gpui::hsla(0., 0., 1., 1.))
            .opacity(if disabled { 0.6 } else { 1.0 })
            .cursor(if disabled {
                CursorStyle::Arrow
            } else {
                CursorStyle::IBeam
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                if !disabled {
                    window.focus(&focus_handle, cx);
                }
            })
            .children(self.prefix)
            .child(self.state)
            .children(self.suffix);
        element.style().refine(&self.style);
        element
    }
}

impl Styled for Input {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
