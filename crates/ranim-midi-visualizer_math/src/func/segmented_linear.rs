#![allow(non_camel_case_types)]

use derive_more::{AsMut, AsRef, Deref, DerefMut, From, Into};
use itertools::Itertools;
use ordered_float::OrderedFloat;
use std::{
    collections::BTreeMap,
    iter::Sum,
    ops::{Add, AddAssign, Bound::*},
};
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

/// Representation of a continuous segmented linear function.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deref, DerefMut, AsRef, AsMut, From, Into)]
pub struct SegmentedLinearFn<X, Y> {
    points: BTreeMap<X, Y>,
}

impl<X, Y> FromIterator<(X, Y)> for SegmentedLinearFn<X, Y>
where
    X: Ord,
{
    fn from_iter<T: IntoIterator<Item = (X, Y)>>(iter: T) -> Self {
        BTreeMap::from_iter(iter).into()
    }
}

pub trait EvalX = Ord + InvertInterpolatable;
pub trait EvalY = Clone + Default + Interpolatable;
pub trait EvalInvX = Ord + Default + Clone + Interpolatable;
pub trait EvalInvY = PartialOrd + InvertInterpolatable;

impl<X, Y> SegmentedLinearFn<X, Y> {
    /// Evaluate the segmented linear function at a given point $x$.
    /// If `extrapolate` is true, extrapolate the function beyond the range of the data points.
    pub fn eval(&self, x: &X, extrapolate: bool) -> Y
    where
        X: EvalX,
        Y: EvalY,
    {
        let mut prev_iter = self.points.range(..x);
        let mut next_iter = self.points.range(x..);
        let prev = prev_iter.next_back();
        let next = next_iter.next();
        match (prev, next) {
            (Some((x1, y1)), Some((x2, y2))) => {
                let t = x.t_value(x1, x2);
                y1.lerp(y2, t)
            }
            (Some((x2, y2)), None) => {
                if extrapolate && let Some((x1, y1)) = prev_iter.next_back() {
                    // extrapolate
                    let t = x.t_value(x1, x2);
                    y1.lerp(y2, t)
                } else {
                    // only one point
                    // treat as constant function
                    y2.clone()
                }
            }
            (None, Some((x1, y1))) => {
                if extrapolate && let Some((x2, y2)) = next_iter.next() {
                    // extrapolate
                    let t = x.t_value(x1, x2);
                    y1.lerp(y2, t)
                } else {
                    // only one point
                    // treat as constant function
                    y1.clone()
                }
            }
            _ => Y::default(),
        }
    }

    /// Evaluate the inverse function of the segmented linear function at a given point $y$.
    /// This requires the current function to be strictly increasing.
    /// If not, the result is undefined.
    ///
    /// The time complexity is $O(\log^2 n)$ due to the use of binary search in finding the range where $x$ is located
    /// in.
    pub fn eval_inv(&self, y: &Y, extrapolate: bool) -> X
    where
        X: Ord + Default + Clone + Interpolatable,
        Y: PartialOrd + InvertInterpolatable,
    {
        if let Some(mut left) = self.first_key_value()
            && let Some(mut right) = self.last_key_value()
        {
            if left.0 == right.0 {
                // Only one point
                return left.0.clone();
            } else if y > right.1 {
                // Right unbounded side
                if extrapolate {
                    left = self.range(..right.0).last().unwrap();
                    let t = y.t_value(left.1, right.1);
                    return left.0.lerp(right.0, t);
                } else {
                    return right.0.clone();
                }
            } else if y < left.1 {
                // Left unbounded side
                if extrapolate {
                    right = self.range((Excluded(left.0), Unbounded)).next().unwrap();
                    let t = y.t_value(left.1, right.1);
                    return left.0.lerp(right.0, t);
                } else {
                    return left.0.clone();
                }
            }
            // In the middle
            // Binary search
            loop {
                let mid_x = left.0.lerp(&right.0, 0.5);

                // mid_x >= left.0
                // so `mid_left` cannot be `None`
                let mid_left = self.range(..=&mid_x).last().unwrap();
                let mid_right = self.range((Excluded(&mid_x), Unbounded)).next().unwrap();
                if y < mid_left.1 {
                    right = mid_left;
                } else if y > mid_right.1 {
                    left = mid_right;
                } else {
                    let t = y.t_value(mid_left.1, mid_right.1);
                    return mid_left.0.lerp(&mid_right.0, t);
                }
            }
        } else {
            // No points
            X::default()
        }
    }

    pub fn into_inv(self) -> SegmentedLinearFn<Y, X> where Y: Ord {
        self.points.into_iter().map(|(x, y)| (y, x)).collect()
    }
}

impl<X, Y> FnOnce<(&X,)> for SegmentedLinearFn<X, Y>
where
    X: EvalX,
    Y: EvalY,
{
    type Output = Y;

    extern "rust-call" fn call_once(self, args: (&X,)) -> Self::Output {
        self.call(args)
    }
}

impl<X, Y> FnMut<(&X,)> for SegmentedLinearFn<X, Y>
where
    X: EvalX,
    Y: EvalY,
{
    extern "rust-call" fn call_mut(&mut self, args: (&X,)) -> Self::Output {
        self.call(args)
    }
}

impl<X, Y> Fn<(&X,)> for SegmentedLinearFn<X, Y>
where
    X: EvalX,
    Y: EvalY,
{
    extern "rust-call" fn call(&self, args: (&X,)) -> Self::Output {
        self.eval(args.0, false)
    }
}

trait XAddRequirement = Ord + Clone + InvertInterpolatable + Default;
trait YAddRequirement = Clone
    + for<'a> AddAssign<&'a Self>
    + for<'a> Add<&'a Self, Output = Self>
    + Interpolatable
    + Default;

impl<X, Y> AddAssign<&Self> for SegmentedLinearFn<X, Y>
where
    X: XAddRequirement,
    Y: YAddRequirement,
{
    fn add_assign(&mut self, rhs: &Self) {
        match rhs.len() {
            0 => (),
            1 => {
                let (x0, y0) = rhs.points.iter().next().unwrap();
                if self.is_empty() {
                    self.points.insert(x0.clone(), y0.clone());
                } else {
                    self.points.iter_mut().for_each(|(_, y)| *y += y0);
                }
            }
            _ => {
                let old_x = rhs
                    .points
                    .iter()
                    .filter(|(x, _)| !self.points.contains_key(x))
                    .map(|(x, y)| (x.clone(), self(x) + y))
                    .collect::<Vec<_>>();
                rhs.points
                    .iter()
                    .tuple_windows()
                    .for_each(|((x1, y1), (x2, y2))| {
                        self.points.range_mut(x1..x2).for_each(|(x, y)| {
                            let t = x.t_value(x1, x2);
                            *y += &y1.lerp(y2, t);
                        });
                    });
                let (x1, y1) = rhs.points.first_key_value().unwrap();
                self.points.range_mut(..x1).for_each(|(_, y)| *y += y1);
                let (x2, y2) = rhs.points.last_key_value().unwrap();
                self.points.range_mut(x2..).for_each(|(_, y)| *y += y2);
                self.points.extend(old_x);
            }
        }
    }
}

impl<X, Y> AddAssign<Self> for SegmentedLinearFn<X, Y>
where
    X: XAddRequirement,
    Y: YAddRequirement,
{
    fn add_assign(&mut self, rhs: Self) {
        self.add_assign(&rhs)
    }
}

impl<X, Y> Add<&Self> for SegmentedLinearFn<X, Y>
where
    X: XAddRequirement,
    Y: YAddRequirement,
{
    type Output = Self;

    fn add(self, rhs: &Self) -> Self {
        let mut res = self.clone();
        res += rhs;
        res
    }
}

impl<X, Y> Add<Self> for SegmentedLinearFn<X, Y>
where
    X: XAddRequirement,
    Y: YAddRequirement,
{
    type Output = Self;

    fn add(self, mut rhs: Self) -> Self {
        match self.len() {
            0 => rhs,
            1 => {
                let y0 = self.points.iter().next().unwrap().1;
                rhs.points.iter_mut().for_each(|(_, y)| *y += y0);
                rhs
            }
            _ => self + &rhs,
        }
    }
}

impl<X, Y> Sum for SegmentedLinearFn<X, Y>
where
    X: XAddRequirement,
    Y: YAddRequirement,
{
    fn sum<I>(iter: I) -> Self
    where
        I: Iterator<Item = Self>,
    {
        iter.fold(Self::default(), Add::add)
    }
}

#[cfg(test)]
mod tests {
    use assert_float_eq::assert_float_absolute_eq;
    use rand::RngExt;
    use tracing::debug;
    use tracing_test::traced_test;

    use super::*;

    #[test]
    fn test_segmented_linear_fn() {
        let f = SegmentedLinearFn::from_iter(
            [(0., 0.), (1., 1.), (2., 1.), (3., 2.), (4., 0.)].map(|(x, y)| (f64o::from(x), y)),
        );
        assert_float_absolute_eq!(f(&f64o::from(-1.)), 0.);
        assert_float_absolute_eq!(f(&f64o::from(0.5)), 0.5);
        assert_float_absolute_eq!(f(&f64o::from(1.5)), 1.);
        assert_float_absolute_eq!(f(&f64o::from(2.5)), 1.5);
        assert_float_absolute_eq!(f(&f64o::from(3.5)), 1.);
        assert_float_absolute_eq!(f(&f64o::from(5.)), 0.);
    }

    #[test]
    fn test_segmented_linear_fn_int() {
        let f = SegmentedLinearFn::from_iter([
            (0u64, 0u64),
            (100, 100),
            (200, 100),
            (300, 200),
            (400, 0),
        ]);
        assert_eq!(f(&50), 50);
        assert_eq!(f(&150), 100);
        assert_eq!(f(&250), 150);
        assert_eq!(f(&350), 100);
        assert_eq!(f(&500), 0);
    }

    #[test]
    fn test_segmented_linear_fn_add() {
        let f = SegmentedLinearFn::from_iter(
            [(0., 0.), (1., 1.), (2., 1.), (3., 2.), (4., 0.)].map(|(x, y)| (f64o::from(x), y)),
        );
        let g = SegmentedLinearFn::from_iter(
            [(-1., 0.), (0., 2.), (5., 1.)].map(|(x, y)| (f64o::from(x), y)),
        );
        let mut h = f.clone();
        h += &g;
        let points = [-1.5, -0.5, 0.5, 1.5, 2.5, 3.5, 4.5, 5.5].map(f64o::from);
        #[allow(clippy::useless_conversion)]
        for x in points.iter() {
            assert!(f64::from(h(x) - (f(x) + g(x))).abs() < 1e-10);
        }
    }

    #[traced_test]
    #[test]
    fn test_eval_inv() {
        let f = SegmentedLinearFn::from_iter(
            [(0., 0.), (1., 1.), (5., 10.), (8., 12.), (20., 15.)].map(|(x, y)| (f64o::from(x), y)),
        );
        let buf = 10.0;
        let min_y = f
            .first_key_value()
            .map(|v| v.1)
            .copied()
            .unwrap_or_default()
            - buf;
        let max_y = f.last_key_value().map(|v| v.1).copied().unwrap_or_default() + buf;

        for _ in 0..100 {
            let orig_y_value = rand::rng().random_range(min_y..max_y);
            let x_value = f.eval_inv(&orig_y_value, true);
            let y_value = f.eval(&x_value, true);
            debug!(
                "f^(-1) ({}) = {}, f({}) = {}",
                orig_y_value, x_value, x_value, y_value
            );
            assert_float_absolute_eq!(y_value, orig_y_value);
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
