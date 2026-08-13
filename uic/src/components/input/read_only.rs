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
    rows: Option<usize>,
    line_height_explicit: bool,
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
            line_height_explicit: false,
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
        self.line_height_explicit = true;
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
        let mut appearance = self.appearance;
        if let Some(rows) = self.rows {
            appearance.multiline_height = appearance.height_for_rows(rows);
        }

        div()
            .flex()
            .when(multiline, |this| this.items_start())
            .when(!multiline, |this| this.items_center())
            .w_full()
            .h(if multiline {
                appearance.multiline_height
            } else {
                appearance.height
            })
            .px(appearance.padding_x)
            .when(multiline, |this| this.py(appearance.padding_y))
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
                    .when(multiline, |this| {
                        this.h_full()
                            .line_height(appearance.line_height)
                            .whitespace_normal()
                    })
                    .overflow_hidden()
                    .child(display_value),
            )
            .children(self.suffix)
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::*;

    #[test]
    fn rows_follow_font_size_unless_line_height_is_explicit() {
        let automatic = ReadOnlyInput::new("").rows(3).font_size(px(20.));
        assert_eq!(automatic.rows, Some(3));
        assert_eq!(automatic.appearance.line_height, px(30.));
        assert_eq!(automatic.appearance.height_for_rows(3), px(112.));

        let explicit = ReadOnlyInput::new("")
            .rows(3)
            .line_height(px(26.))
            .font_size(px(20.));
        assert_eq!(explicit.appearance.line_height, px(26.));
        assert_eq!(explicit.appearance.height_for_rows(3), px(100.));
    }

    #[test]
    fn explicit_height_and_rows_follow_builder_order() {
        let height_wins = ReadOnlyInput::new("").rows(4).multiline_height(px(200.));
        assert_eq!(height_wins.rows, None);
        assert_eq!(height_wins.appearance.multiline_height, px(200.));

        let rows_win = ReadOnlyInput::new("").multiline_height(px(200.)).rows(4);
        assert_eq!(rows_win.rows, Some(4));
    }
}
