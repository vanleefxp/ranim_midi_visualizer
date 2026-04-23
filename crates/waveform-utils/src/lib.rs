#![feature(more_float_constants)]
pub mod envelope;
pub mod synth;
pub mod freq;

use std::f64::consts::{FRAC_1_SQRT_2, PI, SQRT_2, SQRT_3};

const SQRT_6: f64 = SQRT_2 * SQRT_3;

pub trait Waveform {
    /// Evaluate the waveform at time $t$. $t$ is the normalized time in range $[0, 1]$
    /// where 0 is the start of the waveform and 1 is the end of the waveform.
    /// When inputting a $t$ value out of range the result is undefined. Conventionally the waveform function should
    /// have $L^2$ norm of $1/2$, i.e. $integral_0^1 abs(f(t))^2 dif t = 1/2$.
    fn eval(&self, t: f64) -> f64;

    /// Evaluates the waveform as a periodic function. Equivalent to `self.eval(t - t.floor())`.
    #[inline]
    fn eval_cyc(&self, t: f64) -> f64 {
        self.eval(t - t.floor())
    }
}

impl<T: Fn(f64) -> f64> Waveform for T {
    fn eval(&self, t: f64) -> f64 {
        self(t)
    }
}

/// Sine wave.
pub fn sine(t: f64) -> f64 {
    (2. * PI * t).sin()
}

/// Square wave. Maximum is $sqrt(2) / 2$ so that it has $L^2$ norm $1/2$.
pub fn square(t: f64) -> f64 {
    match t {
        ..0.5 => FRAC_1_SQRT_2,
        0.5.. => -FRAC_1_SQRT_2,
        _ => 0.,
    }
}

/// Triangle wave. Maximum is $sqrt(6) / 2$ so that it has $L^2$ norm $1/2$.
pub fn triangle(t: f64) -> f64 {
    match t {
        ..0.25 => 2. * SQRT_6 * t,
        ..0.75 => (1. - 2. * t) * SQRT_6,
        _ => 2. * SQRT_6 * (t - 1.),
    }
}

/// Sawtooth wave. Maximum is $sqrt(6) / 2$ so that it has $L^2$ norm $1/2$.
pub fn sawtooth(t: f64) -> f64 {
    (0.5 - t) * SQRT_6
}

/// Pulse wave. Maximum is $sqrt(2) / 2$ so that it has $L^2$ norm $1/2$.
pub struct Pulse(
    /// The width of the upward pulse in range $[0, 1]$.
    /// 0.5 is equivalent to square wave.
    pub f64,
);

impl Pulse {
    pub const SQUARE: Self = Self(0.5);
}

impl Waveform for Pulse {
    fn eval(&self, t: f64) -> f64 {
        if t <= self.0 {
            FRAC_1_SQRT_2
        } else {
            -FRAC_1_SQRT_2
        }
    }
}
