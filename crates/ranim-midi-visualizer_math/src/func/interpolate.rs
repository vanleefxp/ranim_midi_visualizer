#![allow(non_camel_case_types)]

use ordered_float::OrderedFloat;
use typed_floats::{tf32, tf64};

type f32o = OrderedFloat<f32>;
type f64o = OrderedFloat<f64>;

/// A trait for interpolating to values
///
/// It uses the reference of two values and produce an owned interpolated value.
pub trait Interpolatable {
    /// Lerping between values
    fn lerp(&self, target: &Self, t: f64) -> Self;
}

macro_rules! impl_interpolatable_for_int {
    ($($t:ty),*) => {
        $(
            impl Interpolatable for $t {
                fn lerp(&self, target: &Self, t: f64) -> Self {
                    (*self as f64).lerp(&(*target as f64), t) as $t
                }
            }
        )*
    };
}

impl_interpolatable_for_int!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

impl Interpolatable for f32 {
    fn lerp(&self, target: &Self, t: f64) -> Self {
        self + (target - self) * t as f32
    }
}

impl Interpolatable for f64 {
    fn lerp(&self, target: &Self, t: f64) -> Self {
        self + (target - self) * t
    }
}

impl Interpolatable for f32o {
    fn lerp(&self, target: &Self, t: f64) -> Self {
        self + (target - self) * f32o::from(t as f32)
    }
}

impl Interpolatable for f64o {
    fn lerp(&self, target: &Self, t: f64) -> Self {
        self + (target - self) * f64o::from(t)
    }
}

/// Trait for types that can perform the inverse of interpolation. i.e. find the relative position of a value in the
/// interpolation range.
pub trait InvertInterpolatable {
    /// Returns the relative position (parameter t) of `self` in the interpolation range `start..=end`.
    /// This is the inverse operation of `lerp`.
    fn t_value(&self, start: &Self, end: &Self) -> f64;
}

macro invert_interpolatable_impl($($t:ty),*$(,)?) {
    $(
        impl InvertInterpolatable for $t {
            fn t_value(&self, start: &Self, end: &Self) -> f64 {
                (*self as f64 - *start as f64) / (*end as f64 - *start as f64)
            }
        }
    )*
}

invert_interpolatable_impl!(f32, f64, i8, i16, i32, i64, u8, u16, u32, u64, isize, usize);

impl InvertInterpolatable for f32o {
    fn t_value(&self, start: &Self, end: &Self) -> f64 {
        (*self - *start).0 as f64 / (*end - *start).0 as f64
    }
}

impl InvertInterpolatable for f64o {
    fn t_value(&self, start: &Self, end: &Self) -> f64 {
        (*self - *start).0 / (*end - *start).0
    }
}

impl InvertInterpolatable for tf32::NonNaNFinite {
    fn t_value(&self, start: &Self, end: &Self) -> f64 {
        f32::from(*self - *start) as f64 / f32::from(*end - *start) as f64
    }
}

impl InvertInterpolatable for tf64::NonNaNFinite {
    fn t_value(&self, start: &Self, end: &Self) -> f64 {
        f64::from(*self - *start) / f64::from(*end - *start)
    }
}
