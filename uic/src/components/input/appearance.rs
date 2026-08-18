use gpui::{Hsla, Pixels, hsla, px};

/// Semantic colors and caret geometry unique to editable inputs.
#[derive(Clone, Copy, Debug, uic_macros::Chainable)]
pub struct InputAppearance {
    pub placeholder: Hsla,
    pub focus_border: Hsla,
    pub caret: Hsla,
    pub selection: Hsla,
    pub caret_width: Pixels,
    pub caret_height: Pixels,
}

impl Default for InputAppearance {
    fn default() -> Self {
        Self {
            placeholder: hsla(0., 0., 0.45, 1.),
            focus_border: hsla(0.61, 0.85, 0.55, 1.),
            caret: hsla(0.61, 0.85, 0.55, 1.),
            selection: hsla(0.61, 0.85, 0.55, 0.24),
            caret_width: px(1.5),
            caret_height: px(22.),
        }
    }
}
