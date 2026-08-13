use gpui::{Hsla, Pixels, hsla, px};
#[derive(Clone, Copy, Debug)]
pub struct InputAppearance {
    pub background: Hsla,
    pub foreground: Hsla,
    pub placeholder: Hsla,
    pub border: Hsla,
    pub focus_border: Hsla,
    pub caret: Hsla,
    pub selection: Hsla,
    pub caret_width: Pixels,
    pub caret_height: Pixels,
    pub height: Pixels,
    pub multiline_height: Pixels,
    pub line_height: Pixels,
    pub radius: Pixels,
    pub border_width: Pixels,
    pub padding_x: Pixels,
    pub padding_y: Pixels,
    pub gap: Pixels,
    pub font_size: Pixels,
}

impl Default for InputAppearance {
    fn default() -> Self {
        Self {
            background: hsla(0., 0., 1., 1.),
            foreground: hsla(0., 0., 0.08, 1.),
            placeholder: hsla(0., 0., 0.45, 1.),
            border: hsla(0., 0., 0.75, 1.),
            focus_border: hsla(0.61, 0.85, 0.55, 1.),
            caret: hsla(0.61, 0.85, 0.55, 1.),
            selection: hsla(0.61, 0.85, 0.55, 0.24),
            caret_width: px(1.5),
            caret_height: px(22.),
            height: px(44.),
            multiline_height: px(120.),
            line_height: px(24.),
            radius: px(10.),
            border_width: px(1.),
            padding_x: px(14.),
            padding_y: px(10.),
            gap: px(10.),
            font_size: px(16.),
        }
    }
}

impl InputAppearance {
    pub(crate) fn height_for_rows(self, rows: usize) -> Pixels {
        self.line_height * rows.max(1) as f32 + self.padding_y * 2. + self.border_width * 2.
    }
}
