use std::{
    any::Any,
    f64::consts::{FRAC_1_SQRT_2, PI, SQRT_2, SQRT_3},
};

pub(crate) const SQRT_6: f64 = SQRT_2 * SQRT_3;

pub trait Waveform: Any {
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

impl<T: Fn(f64) -> f64 + 'static> Waveform for T {
    fn eval(&self, t: f64) -> f64 {
        self(t)
    }
}

/// Automatically generate [`FnMut`] and [`FnOnce`] implementations for types implemented [`Fn`].
// [TODO] maybe make this a derive macro or proc macro?
pub(crate) macro auto_fn_impl($t: ty, $args: ty, $output: ty$(,)?) {
    impl FnMut<$args> for $t {
        extern "rust-call" fn call_mut(&mut self, args: $args) -> Self::Output {
            self.call(args)
        }
    }
    impl FnOnce<$args> for $t {
        type Output = $output;
        extern "rust-call" fn call_once(self, args: $args) -> Self::Output {
            self.call(args)
        }
    }
}

/// Sine wave.
pub fn sine(t: f64) -> f64 {
    (2. * PI * t).sin()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sine;

auto_fn_impl!(Sine, (f64,), f64);
impl Fn<(f64,)> for Sine {
    extern "rust-call" fn call(&self, args: (f64,)) -> Self::Output {
        sine(args.0)
    }
}

/// Square wave. Maximum is $sqrt(2) / 2$ so that it has $L^2$ norm $1/2$.
pub fn square(t: f64) -> f64 {
    match t {
        ..0.5 => FRAC_1_SQRT_2,
        0.5.. => -FRAC_1_SQRT_2,
        _ => 0.,
    }
}

pub struct Square;

auto_fn_impl!(Square, (f64,), f64);
impl Fn<(f64,)> for Square {
    extern "rust-call" fn call(&self, args: (f64,)) -> Self::Output {
        square(args.0)
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

pub struct Triangle;

auto_fn_impl!(Triangle, (f64,), f64);
impl Fn<(f64,)> for Triangle {
    extern "rust-call" fn call(&self, args: (f64,)) -> Self::Output {
        triangle(args.0)
    }
}

/// Sawtooth wave. Maximum is $sqrt(6) / 2$ so that it has $L^2$ norm $1/2$.
pub fn sawtooth(t: f64) -> f64 {
    (0.5 - t) * SQRT_6
}

pub struct Sawtooth;

auto_fn_impl!(Sawtooth, (f64,), f64);
impl Fn<(f64,)> for Sawtooth {
    extern "rust-call" fn call(&self, args: (f64,)) -> Self::Output {
        sawtooth(args.0)
    }
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

impl Fn<(f64,)> for Pulse {
    extern "rust-call" fn call(&self, args: (f64,)) -> Self::Output {
        if args.0 <= self.0 {
            FRAC_1_SQRT_2
        } else {
            -FRAC_1_SQRT_2
        }
    }
}

impl FnMut<(f64,)> for Pulse {
    extern "rust-call" fn call_mut(&mut self, args: (f64,)) -> Self::Output {
        self.call(args)
    }
}

impl FnOnce<(f64,)> for Pulse {
    type Output = f64;
    extern "rust-call" fn call_once(self, args: (f64,)) -> Self::Output {
        self.call(args)
    }
}
