use crate::{Pixels, Rems};

const CRITICAL_DAMPING_TOLERANCE: f32 = 1e-4;

/// Physical parameters for a damped harmonic oscillator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringConfig {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}

impl SpringConfig {
    pub const fn new(stiffness: f32, damping: f32, mass: f32) -> Self {
        Self { stiffness, damping, mass }
    }

    pub fn canonical(&self) -> (f32, f32) {
        let natural_frequency = (self.stiffness / self.mass).sqrt();
        let damping_ratio = self.damping / (2.0 * (self.stiffness * self.mass).sqrt());
        (natural_frequency, damping_ratio)
    }

    /// Advances the oscillator exactly for a fixed target and elapsed time.
    pub fn step(&self, state: SpringState, target: f32, delta_time: f32) -> SpringState {
        let matrix = self.propagator(delta_time.max(0.0));
        let displacement = state.position - target;
        SpringState {
            position: target + matrix[0][0] * displacement + matrix[0][1] * state.velocity,
            velocity: matrix[1][0] * displacement + matrix[1][1] * state.velocity,
        }
    }

    fn propagator(&self, delta_time: f32) -> [[f32; 2]; 2] {
        let (frequency, ratio) = self.canonical();
        if !frequency.is_finite() || frequency <= 0.0 || !ratio.is_finite() {
            return [[1.0, 0.0], [0.0, 1.0]];
        }
        if ratio < 1.0 - CRITICAL_DAMPING_TOLERANCE {
            let decay = ratio * frequency;
            let damped = frequency * (1.0 - ratio * ratio).sqrt();
            let exponential = (-decay * delta_time).exp();
            let (sine, cosine) = (damped * delta_time).sin_cos();
            let sine_over_frequency = sine / damped;
            [
                [
                    exponential * (cosine + decay * sine_over_frequency),
                    exponential * sine_over_frequency,
                ],
                [
                    -exponential * frequency * frequency * sine_over_frequency,
                    exponential * (cosine - decay * sine_over_frequency),
                ],
            ]
        } else if ratio > 1.0 + CRITICAL_DAMPING_TOLERANCE {
            let root = (ratio * ratio - 1.0).sqrt();
            let root_sum = ratio + root;
            let slow = -frequency / root_sum;
            let fast = -frequency * root_sum;
            let denominator = slow - fast;
            let slow_exponential = (slow * delta_time).exp();
            let fast_exponential = (fast * delta_time).exp();
            [
                [
                    (-fast * slow_exponential + slow * fast_exponential) / denominator,
                    (slow_exponential - fast_exponential) / denominator,
                ],
                [
                    slow * fast * (fast_exponential - slow_exponential) / denominator,
                    (slow * slow_exponential - fast * fast_exponential) / denominator,
                ],
            ]
        } else {
            let exponential = (-frequency * delta_time).exp();
            [
                [
                    exponential * (1.0 + frequency * delta_time),
                    exponential * delta_time,
                ],
                [
                    -exponential * frequency * frequency * delta_time,
                    exponential * (1.0 - frequency * delta_time),
                ],
            ]
        }
    }

    pub fn is_settled(&self, state: SpringState, target: f32, epsilon: f32) -> bool {
        let (frequency, _) = self.canonical();
        epsilon.is_finite()
            && epsilon >= 0.0
            && (state.position - target).abs() <= epsilon
            && state.velocity.abs() <= epsilon * frequency
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpringState {
    pub position: f32,
    pub velocity: f32,
}

pub trait SpringTarget: 'static {
    type Output;
    fn target(&self) -> f32;
    fn resolve(&self, value: f32) -> Self::Output;
}

impl SpringTarget for f32 {
    type Output = f32;
    fn target(&self) -> f32 { *self }
    fn resolve(&self, value: f32) -> Self::Output { value }
}

impl SpringTarget for Pixels {
    type Output = Pixels;
    fn target(&self) -> f32 { self.as_f32() }
    fn resolve(&self, value: f32) -> Self::Output { Pixels::from(value) }
}

impl SpringTarget for Rems {
    type Output = Rems;
    fn target(&self) -> f32 { self.0 }
    fn resolve(&self, value: f32) -> Self::Output { Rems(value) }
}
