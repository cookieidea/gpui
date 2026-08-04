use gpui::Rgba;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsva {
    pub h: f32,
    pub s: f32,
    pub v: f32,
    pub a: f32,
}

impl Default for Hsva {
    fn default() -> Self {
        Self {
            h: 0.0,
            s: 0.0,
            v: 0.0,
            a: 1.0,
        }
    }
}

impl Hsva {
    pub fn new(h: f32, s: f32, v: f32, a: f32) -> Self {
        Self {
            h: h.clamp(0.0, 1.0),
            s: s.clamp(0.0, 1.0),
            v: v.clamp(0.0, 1.0),
            a: a.clamp(0.0, 1.0),
        }
    }

    pub fn from_rgba_preserving_hue(color: Rgba, fallback_hue: f32) -> Self {
        let max = color.r.max(color.g).max(color.b);
        let min = color.r.min(color.g).min(color.b);
        let delta = max - min;
        let saturation = if max <= f32::EPSILON {
            0.0
        } else {
            delta / max
        };
        let hue = if delta <= f32::EPSILON {
            fallback_hue.clamp(0.0, 1.0)
        } else if max == color.r {
            ((color.g - color.b) / delta).rem_euclid(6.0) / 6.0
        } else if max == color.g {
            ((color.b - color.r) / delta + 2.0) / 6.0
        } else {
            ((color.r - color.g) / delta + 4.0) / 6.0
        };

        Self::new(hue, saturation, max, color.a)
    }

    pub fn to_rgba(self) -> Rgba {
        let hue = self.h.rem_euclid(1.0) * 6.0;
        let chroma = self.v * self.s;
        let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
        let (r, g, b) = match hue.floor() as u8 {
            0 | 6 => (chroma, x, 0.0),
            1 => (x, chroma, 0.0),
            2 => (0.0, chroma, x),
            3 => (0.0, x, chroma),
            4 => (x, 0.0, chroma),
            _ => (chroma, 0.0, x),
        };
        let match_value = self.v - chroma;
        Rgba {
            r: r + match_value,
            g: g + match_value,
            b: b + match_value,
            a: self.a,
        }
    }
}

impl From<Rgba> for Hsva {
    fn from(value: Rgba) -> Self {
        Self::from_rgba_preserving_hue(value, 0.0)
    }
}

impl From<Hsva> for Rgba {
    fn from(value: Hsva) -> Self {
        value.to_rgba()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_channel(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
    }

    #[test]
    fn primary_colors_round_trip_through_hsva() {
        for color in [
            Rgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            Rgba {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 0.5,
            },
            Rgba {
                r: 0.0,
                g: 0.0,
                b: 1.0,
                a: 0.25,
            },
        ] {
            let actual = Hsva::from(color).to_rgba();
            assert_channel(actual.r, color.r);
            assert_channel(actual.g, color.g);
            assert_channel(actual.b, color.b);
            assert_channel(actual.a, color.a);
        }
    }

    #[test]
    fn achromatic_colors_preserve_the_previous_hue() {
        let color = Rgba {
            r: 0.4,
            g: 0.4,
            b: 0.4,
            a: 1.0,
        };
        let hsva = Hsva::from_rgba_preserving_hue(color, 0.625);
        assert_channel(hsva.h, 0.625);
        assert_channel(hsva.s, 0.0);
        assert_channel(hsva.v, 0.4);
    }
}
