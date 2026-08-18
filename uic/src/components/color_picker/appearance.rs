use gpui::{Hsla, Pixels, hsla, px};

/// Semantic focus color used by the picker controls.
#[derive(Clone, Copy, Debug, uic_macros::Chainable)]
pub struct ColorPickerAppearance {
    pub accent: Hsla,
    pub marker: Hsla,
    pub area_height: Pixels,
    pub hue_width: Pixels,
    pub marker_size: Pixels,
}

impl Default for ColorPickerAppearance {
    fn default() -> Self {
        Self {
            accent: hsla(0.52, 0.85, 0.48, 1.0),
            marker: hsla(0.0, 0.0, 1.0, 1.0),
            area_height: px(320.0),
            hue_width: px(34.0),
            marker_size: px(18.0),
        }
    }
}
