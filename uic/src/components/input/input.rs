use gpui::{
    AnyElement, App, CursorStyle, Entity, IntoElement, MouseButton, RenderOnce, Window, div,
    prelude::*,
};

use super::{InputAppearance, TextInput};

#[derive(IntoElement)]
pub struct Input {
    state: Entity<TextInput>,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    appearance: InputAppearance,
}

input_appearance!(Input);

impl Input {
    pub fn new(state: &Entity<TextInput>) -> Self {
        Self {
            state: state.clone(),
            prefix: None,
            suffix: None,
            appearance: InputAppearance::default(),
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

        let (focused, focus_handle, disabled) = {
            let state = self.state.read(cx);
            (
                state.focus_handle.is_focused(window),
                state.focus_handle.clone(),
                state.disabled,
            )
        };

        div()
            .flex()
            .items_center()
            .w_full()
            .h(appearance.height)
            .px(appearance.padding_x)
            .gap(appearance.gap)
            .rounded(appearance.radius)
            .border(appearance.border_width)
            .border_color(if focused && !disabled {
                appearance.focus_border
            } else {
                appearance.border
            })
            .bg(appearance.background)
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
            .children(self.suffix)
    }
}
