use std::ops::RangeInclusive;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NumericRange {
    min: f64,
    max: f64,
}

impl NumericRange {
    pub(crate) fn new(range: RangeInclusive<f64>) -> Self {
        let min = *range.start();
        let max = *range.end();
        assert!(
            min.is_finite() && max.is_finite(),
            "range bounds must be finite"
        );
        assert!(min <= max, "range start must not exceed its end");
        Self { min, max }
    }

    pub(crate) fn min(self) -> f64 {
        self.min
    }

    pub(crate) fn max(self) -> f64 {
        self.max
    }

    pub(crate) fn span(self) -> f64 {
        self.max - self.min
    }

    pub(crate) fn clamp(self, value: f64) -> f64 {
        if value.is_finite() {
            value.clamp(self.min, self.max)
        } else {
            self.min
        }
    }

    pub(crate) fn snap(self, value: f64, step: f64) -> f64 {
        let value = self.clamp(value);
        if !step.is_finite() || step <= 0.0 || self.span() <= f64::EPSILON {
            return value;
        }
        self.clamp(self.min + ((value - self.min) / step).round() * step)
    }

    pub(crate) fn ratio(self, value: f64) -> f32 {
        if self.span() <= f64::EPSILON {
            return 0.0;
        }
        ((self.clamp(value) - self.min) / self.span()) as f32
    }

    pub(crate) fn value_at(self, ratio: f32) -> f64 {
        self.min + self.span() * f64::from(ratio.clamp(0.0, 1.0))
    }

    pub(crate) fn as_inclusive(self) -> RangeInclusive<f64> {
        self.min..=self.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_normalizes_and_snaps_from_the_range_start() {
        let range = NumericRange::new(-10.0..=30.0);
        assert_eq!(range.ratio(-10.0), 0.0);
        assert_eq!(range.ratio(10.0), 0.5);
        assert_eq!(range.ratio(50.0), 1.0);
        assert_eq!(range.value_at(0.25), 0.0);
        assert_eq!(range.snap(6.4, 4.0), 6.0);
    }
}
