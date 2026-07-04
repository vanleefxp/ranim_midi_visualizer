#![allow(non_camel_case_types)]

use super::interpolate::{Interpolatable, InvertInterpolatable};
use derive_more::{AsMut, AsRef, Deref, DerefMut, From, Into};
use itertools::Itertools;
use std::{
    collections::BTreeMap,
    iter::Sum,
    ops::{
        Add, AddAssign,
        Bound::{self, *},
        RangeBounds,
    },
};

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

    /// Evaluate the sigmented linear function for a range bound.
    /// The result is the function value at the point with the same inclusiveness of the original range.
    fn eval_bound(&self, x_bound: Bound<&X>, extrapolate: bool) -> Bound<Y>
    where
        X: EvalX,
        Y: EvalY,
    {
        match x_bound {
            Unbounded => Unbounded,
            Included(x) => Included(self.eval(x, extrapolate)),
            Excluded(x) => Excluded(self.eval(x, extrapolate)),
        }
    }

    /// Find the range of $y$ corresponding to that of $x$.
    /// This works only for strictly increasing functions (which is invertible).
    /// If not, the result is undefined.
    pub fn y_range(&self, x_range: impl RangeBounds<X>, extrapolate: bool) -> (Bound<Y>, Bound<Y>)
    where
        X: EvalX,
        Y: EvalY,
    {
        (
            self.eval_bound(x_range.start_bound(), extrapolate),
            self.eval_bound(x_range.end_bound(), extrapolate),
        )
    }

    /// Evaluate the inverse function of the segmented linear function at a given point $y$.
    /// This requires the current function to be strictly increasing (which makes it invertible).
    /// If not, the result is undefined.
    ///
    /// The time complexity is $O(\log^2 n)$ due to the use of binary search in finding the range where $x$ is located
    /// in.
    pub fn eval_inv(&self, y: &Y, extrapolate: bool) -> X
    where
        X: EvalInvX,
        Y: EvalInvY,
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
                let mid_x = left.0.lerp(right.0, 0.5);

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
                    return mid_left.0.lerp(mid_right.0, t);
                }
            }
        } else {
            // No points
            X::default()
        }
    }

    fn eval_bound_inv(&self, y_bound: Bound<&Y>, extrapolate: bool) -> Bound<X>
    where
        X: EvalInvX,
        Y: EvalInvY,
    {
        match y_bound {
            Unbounded => Unbounded,
            Included(y) => Included(self.eval_inv(y, extrapolate)),
            Excluded(y) => Excluded(self.eval_inv(y, extrapolate)),
        }
    }

    /// Find the range of $x$ corresponding to that of $y$.
    /// This works only for strictly increasing functions (which is invertible).
    /// If not, the result is undefined.
    pub fn x_range(&self, y_range: impl RangeBounds<Y>, extrapolate: bool) -> (Bound<X>, Bound<X>)
    where
        X: EvalInvX,
        Y: EvalInvY,
    {
        (
            self.eval_bound_inv(y_range.start_bound(), extrapolate),
            self.eval_bound_inv(y_range.end_bound(), extrapolate),
        )
    }

    pub fn into_inv(self) -> SegmentedLinearFn<Y, X>
    where
        Y: Ord,
    {
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
    use super::*;
    use assert_float_eq::assert_float_absolute_eq;
    use ordered_float::OrderedFloat;
    use rand::RngExt;
    use tracing::debug;
    use tracing_test::traced_test;

    type f64o = OrderedFloat<f64>;

    #[test]
    fn test_segmented_linear_fn() {
        let f = SegmentedLinearFn::from_iter(
            [(0., 0.), (1., 1.), (2., 1.), (3., 2.), (4., 0.)].map(|(x, y)| (f64o::from(x), y)),
        );
        assert_float_absolute_eq!(f(&f64o::from(-1.)), 0.);
        assert_float_absolute_eq!(f.eval(&f64o::from(-1.), true), -1.);
        assert_float_absolute_eq!(f(&f64o::from(0.5)), 0.5);
        assert_float_absolute_eq!(f(&f64o::from(1.5)), 1.);
        assert_float_absolute_eq!(f(&f64o::from(2.5)), 1.5);
        assert_float_absolute_eq!(f(&f64o::from(3.5)), 1.);
        assert_float_absolute_eq!(f(&f64o::from(5.)), 0.);
        assert_float_absolute_eq!(f.eval(&f64o::from(5.), true), -2.);
    }

    #[test]
    fn test_segmented_linear_fn_int() {
        let f = SegmentedLinearFn::from_iter([
            (0i64, 0i64),
            (100, 100),
            (200, 100),
            (300, 200),
            (400, 0),
        ]);
        assert_eq!(f(&-100), 0);
        assert_eq!(f.eval(&-100, true), -100);
        assert_eq!(f(&50), 50);
        assert_eq!(f(&150), 100);
        assert_eq!(f(&250), 150);
        assert_eq!(f(&350), 100);
        assert_eq!(f(&500), 0);
        assert_eq!(f.eval(&500, true), -200);
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
            assert_float_absolute_eq!(h(x), f(x) + g(x));
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
