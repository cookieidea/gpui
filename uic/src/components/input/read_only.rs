use super::{InputAppearance, InputMode};
use gpui::{
    AnyElement, CursorStyle, IntoElement, Refineable as _, RenderOnce, SharedString,
    StyleRefinement, Styled, div, prelude::*,
};
/// An input-shaped value display without focus or editing state.
#[derive(IntoElement)]
pub struct ReadOnlyInput {
    value: SharedString,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    mode: InputMode,
    appearance: InputAppearance,
    rows: Option<usize>,
    style: StyleRefinement,
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

    pub fn multiline(mut self) -> Self {
        self.mode = InputMode::Multiline;
        self
    }

    pub fn appearance(mut self, appearance: InputAppearance) -> Self {
        self.appearance = appearance;
        self
    }
}

impl RenderOnce for ReadOnlyInput {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        let multiline = self.mode == InputMode::Multiline;
        let display_value = match self.mode {
            InputMode::Text | InputMode::Multiline => self.value,
            InputMode::Password => "•".repeat(self.value.chars().count()).into(),
        };
        let row_height = self
            .rows
            .map(|rows| super::row_height(&self.style, rows, _window.rem_size()));

        let mut element = div()
            .flex()
            .when(multiline, |this| this.items_start())
            .when(!multiline, |this| this.items_center())
            .w_full()
            .h(gpui::px(44.))
            .when_some(row_height.filter(|_| multiline), |this, height| {
                this.h(height)
            })
            .px(gpui::px(14.))
            .when(multiline, |this| this.py(gpui::px(10.)))
            .gap(gpui::px(10.))
            .rounded(gpui::px(10.))
            .border(gpui::px(1.))
            .border_color(gpui::hsla(0., 0., 0.75, 1.))
            .bg(gpui::hsla(0., 0., 1., 1.))
            .text_size(gpui::px(16.))
            .line_height(gpui::px(24.))
            .text_color(gpui::hsla(0., 0., 0.08, 1.))
            .cursor(CursorStyle::Arrow)
            .children(self.prefix)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .when(multiline, |this| this.h_full().whitespace_normal())
                    .overflow_hidden()
                    .child(display_value),
            )
            .children(self.suffix);
        element.style().refine(&self.style);
        element
    }
}

impl Styled for ReadOnlyInput {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::*;

    #[test]
    fn rows_are_independent_from_the_semantic_appearance() {
        let automatic = ReadOnlyInput::new("").rows(3).text_size(px(20.));
        assert_eq!(automatic.rows, Some(3));
        assert!(automatic.style.text.font_size.is_some());
    }

    #[test]
    fn explicit_height_is_stored_in_styled() {
        let input = ReadOnlyInput::new("").rows(4).h(px(200.));
        assert_eq!(input.rows, Some(4));
        assert!(input.style.size.height.is_some());
    }
}
