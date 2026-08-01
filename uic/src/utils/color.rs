use gpui::{Fill, Hsla, solid_background};

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, strum::Display)]
pub enum InvalidHexColorError {
    #[strum(to_string = "invalid red channel")]
    InvalidRed,

    #[strum(to_string = "invalid green channel")]
    InvalidGreen,

    #[strum(to_string = "invalid blue channel")]
    InvalidBlue,

    #[strum(to_string = "invalid alpha channel")]
    InvalidAlpha,

    #[strum(to_string = "invalid color length: {0} ")]
    InvalidColorLength(usize),
}

pub trait HslaColor {
    fn hsla(&self) -> Hsla;
    fn try_hsla(&self) -> Result<Hsla, InvalidHexColorError>;
    fn fill(&self) -> Fill {
        Fill::Color(solid_background(self.hsla()))
    }
}

impl<T: AsRef<str>> HslaColor for T {
    fn hsla(&self) -> Hsla {
        hex_to_hsla(self.as_ref())
    }
    fn try_hsla(&self) -> Result<Hsla, InvalidHexColorError> {
        try_hex_to_hsla(self.as_ref())
    }
}

fn try_hex_to_hsla(hex: &str) -> Result<Hsla, InvalidHexColorError> {
    let mut hex = hex.trim().trim_start_matches('#').to_string();

    // 支持 #RGB / #RGBA 简写
    if hex.len() == 3 || hex.len() == 4 {
        hex = hex.chars().flat_map(|c| [c, c]).collect::<String>();
    }

    if hex.len() != 6 && hex.len() != 8 {
        return Err(InvalidHexColorError::InvalidColorLength(hex.len()));
    }

    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| InvalidHexColorError::InvalidRed)?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| InvalidHexColorError::InvalidGreen)?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| InvalidHexColorError::InvalidBlue)?;

    let a = if hex.len() == 8 {
        let alpha =
            u8::from_str_radix(&hex[6..8], 16).map_err(|_| InvalidHexColorError::InvalidAlpha)?;

        alpha as f32 / 255.0
    } else {
        1.0
    };

    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);

    let l = (max + min) / 2.0;
    let d = max - min;

    let mut h = 0.0;
    let mut s = 0.0;

    if d != 0.0 {
        s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };

        h = if max == r {
            ((g - b) / d + if g < b { 6.0 } else { 0.0 }) * 60.0
        } else if max == g {
            ((b - r) / d + 2.0) * 60.0
        } else {
            ((r - g) / d + 4.0) * 60.0
        } / 360.0;
    }

    Ok(Hsla { h, s, l, a })
}

fn hex_to_hsla(hex: &str) -> Hsla {
    try_hex_to_hsla(hex).unwrap_or_default()
}
