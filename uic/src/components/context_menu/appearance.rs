use gpui::{Hsla, Pixels, px, rgb};

/// Visual defaults used when a context menu does not supply a custom surface.
#[derive(Clone, Copy, Debug)]
pub struct ContextMenuAppearance {
    pub background: Hsla,
    pub foreground: Hsla,
    pub muted_foreground: Hsla,
    pub danger_foreground: Hsla,
    pub selected_background: Hsla,
    pub selected_foreground: Hsla,
    pub border: Hsla,
    pub border_width: Pixels,
    pub radius: Pixels,
    pub width: Pixels,
    pub max_height: Pixels,
    pub padding: Pixels,
    pub item_height: Pixels,
    pub item_padding_x: Pixels,
    pub item_radius: Pixels,
    pub separator: Hsla,
    pub separator_margin: Pixels,
    pub window_margin: Pixels,
    pub submenu_gap: Pixels,
}

impl Default for ContextMenuAppearance {
    fn default() -> Self {
        Self {
            background: rgb(0xffffff).into(),
            foreground: rgb(0x172033).into(),
            muted_foreground: rgb(0x7b8496).into(),
            danger_foreground: rgb(0xdc2626).into(),
            selected_background: rgb(0xe9eef8).into(),
            selected_foreground: rgb(0x172033).into(),
            border: rgb(0x000000).opacity(0.12).into(),
            border_width: px(1.),
            radius: px(10.),
            width: px(220.),
            max_height: px(420.),
            padding: px(5.),
            item_height: px(34.),
            item_padding_x: px(10.),
            item_radius: px(6.),
            separator: rgb(0x000000).opacity(0.1).into(),
            separator_margin: px(5.),
            window_margin: px(8.),
            submenu_gap: px(2.),
        }
    }
}
