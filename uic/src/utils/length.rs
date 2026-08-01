use gpui::{AbsoluteLength, DefiniteLength, Pixels, Rems};

pub fn fraction(frac: f32) -> DefiniteLength {
    gpui::DefiniteLength::Fraction(frac)
}

pub fn absolute_px(pixels: f32) -> DefiniteLength {
    gpui::DefiniteLength::Absolute(AbsoluteLength::Pixels(gpui::px(pixels)))
}

pub fn absolute_rems(rems: f32) -> DefiniteLength {
    gpui::DefiniteLength::Absolute(AbsoluteLength::Rems(gpui::rems(rems)))
}

pub trait DefiniteLengthAbsoluteExt {
    fn absolute(&self) -> DefiniteLength;
}

impl DefiniteLengthAbsoluteExt for Pixels {
    fn absolute(&self) -> DefiniteLength {
        gpui::DefiniteLength::Absolute(AbsoluteLength::Pixels(*self))
    }
}

impl DefiniteLengthAbsoluteExt for Rems {
    fn absolute(&self) -> DefiniteLength {
        gpui::DefiniteLength::Absolute(AbsoluteLength::Rems(*self))
    }
}
