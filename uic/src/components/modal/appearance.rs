use gpui::{Hsla, Pixels, hsla, px};

#[derive(Clone, Copy, Debug, uic_macros::Chainable)]
pub struct ModalButtonAppearance {
    pub background: Hsla,
    pub foreground: Hsla,
    pub border: Hsla,
    pub hover_background: Hsla,
    pub border_width: Pixels,
    pub radius: Pixels,
    pub height: Pixels,
    pub padding_x: Pixels,
}

/// Appearance of the modal's internal sections and default controls.
/// The panel itself is styled through `Styled` on `Modal`.
#[derive(Clone, Copy, Debug, uic_macros::Chainable)]
pub struct ModalAppearance {
    pub backdrop: Hsla,
    pub section_borders: bool,
    pub header_padding_x: Pixels,
    pub header_padding_y: Pixels,
    pub body_padding_x: Pixels,
    pub body_padding_y: Pixels,
    pub footer_padding_x: Pixels,
    pub footer_padding_y: Pixels,
    pub section_border: Hsla,
    pub footer_gap: Pixels,
    pub ok_button: ModalButtonAppearance,
    pub cancel_button: ModalButtonAppearance,
}

impl Default for ModalAppearance {
    fn default() -> Self {
        Self {
            backdrop: hsla(0., 0., 0., 0.45),
            section_borders: true,
            header_padding_x: px(24.),
            header_padding_y: px(10.),
            body_padding_x: px(24.),
            body_padding_y: px(16.),
            footer_padding_x: px(24.),
            footer_padding_y: px(12.),
            section_border: hsla(0., 0., 0., 0.08),
            footer_gap: px(10.),
            ok_button: ModalButtonAppearance {
                background: hsla(0.61, 0.85, 0.55, 1.),
                foreground: hsla(0., 0., 1., 1.),
                border: hsla(0.61, 0.85, 0.55, 1.),
                hover_background: hsla(0.61, 0.75, 0.48, 1.),
                border_width: px(1.),
                radius: px(8.),
                height: px(36.),
                padding_x: px(16.),
            },
            cancel_button: ModalButtonAppearance {
                background: hsla(0., 0., 1., 1.),
                foreground: hsla(0., 0., 0.16, 1.),
                border: hsla(0., 0., 0., 0.18),
                hover_background: hsla(0., 0., 0.96, 1.),
                border_width: px(1.),
                radius: px(8.),
                height: px(36.),
                padding_x: px(16.),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::rgba;

    #[test]
    fn appearance_fields_support_chainable_construction() {
        let defaults = ModalAppearance::default();
        let appearance = defaults.backdrop(rgba(0x080a10a8).into()).ok_button(
            defaults
                .ok_button
                .background(rgba(0x171a24f2).into())
                .foreground(rgba(0xf4f4f7f2).into()),
        );

        assert_eq!(appearance.backdrop, rgba(0x080a10a8).into());
        assert_eq!(appearance.ok_button.background, rgba(0x171a24f2).into());
        assert_eq!(appearance.ok_button.foreground, rgba(0xf4f4f7f2).into());
    }
}
