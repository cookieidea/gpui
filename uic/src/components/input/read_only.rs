use super::{InputAppearance, InputMode};
use gpui::{AnyElement, CursorStyle, IntoElement, RenderOnce, SharedString, div, prelude::*};
/// An input-shaped value display without focus or editing state.
#[derive(IntoElement)]
pub struct ReadOnlyInput {
    value: SharedString,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    mode: InputMode,
    appearance: InputAppearance,
}

input_appearance!(ReadOnlyInput);

impl ReadOnlyInput {
    pub fn new(value: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            prefix: None,
            suffix: None,
            mode: InputMode::Text,
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

    pub fn mode(mut self, mode: InputMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn text(mut self) -> Self {
        self.mode = InputMode::Text;
        self
    }

    pub fn password(mut self) -> Self {
        self.mode = InputMode::Password;
        self
    }

    pub fn appearance(mut self, appearance: InputAppearance) -> Self {
        self.appearance = appearance;
        self
    }
}

impl RenderOnce for ReadOnlyInput {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        let display_value = match self.mode {
            InputMode::Text => self.value,
            InputMode::Password => "•".repeat(self.value.chars().count()).into(),
        };
        let appearance = self.appearance;

        div()
            .flex()
            .items_center()
            .w_full()
            .h(appearance.height)
            .px(appearance.padding_x)
            .gap(appearance.gap)
            .rounded(appearance.radius)
            .border(appearance.border_width)
            .border_color(appearance.border)
            .bg(appearance.background)
            .text_size(appearance.font_size)
            .text_color(appearance.foreground)
            .cursor(CursorStyle::Arrow)
            .children(self.prefix)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(display_value),
            )
            .children(self.suffix)
    }
}
