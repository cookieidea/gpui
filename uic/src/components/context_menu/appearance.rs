use gpui::{Hsla, Pixels, px, rgb};

/// Visual defaults used when a context menu does not supply a custom surface.
#[derive(Clone, Copy, Debug, uic_macros::Chainable)]
pub struct ContextMenuAppearance {
    pub muted_foreground: Hsla,
    pub danger_foreground: Hsla,
    pub selected_background: Hsla,
    pub selected_foreground: Hsla,
    pub item_height: Pixels,
    pub item_padding_x: Pixels,
    pub item_radius: Pixels,
    pub separator: Hsla,
    pub separator_margin: Pixels,
}

impl Default for ContextMenuAppearance {
    fn default() -> Self {
        Self {
            muted_foreground: rgb(0x7b8496).into(),
            danger_foreground: rgb(0xdc2626).into(),
            selected_background: rgb(0xe9eef8).into(),
            selected_foreground: rgb(0x172033).into(),
            item_height: px(34.),
            item_padding_x: px(10.),
            item_radius: px(6.),
            separator: rgb(0x000000).opacity(0.1).into(),
            separator_margin: px(5.),
        }
    }
}
