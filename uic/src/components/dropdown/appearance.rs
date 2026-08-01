use gpui::{Hsla, Pixels, px};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DropdownPlacement {
    #[default]
    BottomStart,
    BottomEnd,
    TopStart,
    TopEnd,
}

#[derive(Clone, Copy, Debug)]
pub struct DropdownAppearance {
    pub background: Hsla,
    pub border: Hsla,
    pub border_width: Pixels,
    pub radius: Pixels,
    pub padding: Pixels,
    pub gap: Pixels,
    pub min_width: Pixels,
    pub max_height: Pixels,
}

impl Default for DropdownAppearance {
    fn default() -> Self {
        Self {
            background: gpui::white(),
            border: gpui::black().opacity(0.12),
            border_width: px(1.),
            radius: px(8.),
            padding: px(6.),
            gap: px(6.),
            min_width: px(160.),
            max_height: px(320.),
        }
    }
}
