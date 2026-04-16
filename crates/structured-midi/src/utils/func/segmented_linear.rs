#![allow(non_camel_case_types)]

use derive_more::{AsMut, AsRef, Deref, DerefMut, From, Into};
use itertools::Itertools;
use ordered_float::OrderedFloat;
use ranim::prelude::Interpolatable;
use std::{
    collections::BTreeMap,
    iter::Sum,
    ops::{Add, AddAssign},
};

type f32o = OrderedFloat<f32>;
type f64o = OrderedFloat<f64>;

pub trait InvertInterpolatable {
    fn t_value(&self, start: &Self, end: &Self) -> f64;
}

macro_rules! impl_invert_interpolatable {
    ($($t:ty),*$(,)?) => {
        $(
            impl InvertInterpolatable for $t {
                fn t_value(&self, start: &Self, end: &Self) -> f64 {
                    (*self as f64 - *start as f64) / (*end as f64 - *start as f64)
                }
            }
        )*
    };
}

impl_invert_interpolatable!(f32, f64, i8, i16, i32, i64, u8, u16, u32, u64, isize, usize);

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

trait XCallRequirement = Ord + InvertInterpolatable;
trait YCallRequirement = Interpolatable + Clone + Default;

impl<X, Y> SegmentedLinearFn<X, Y> {
    #[inline(always)]
    fn _call(&self, x: &X) -> Y
    where
        X: XCallRequirement,
        Y: YCallRequirement,
    {
        let prev = self.points.range(..x).next_back();
        let next = self.points.range(x..).next();
        match (prev, next) {
            (Some((x1, y1)), Some((x2, y2))) => {
                let t = x.t_value(x1, x2);
                y1.lerp(y2, t)
            }
            (Some((_, y0)), None) | (None, Some((_, y0))) => y0.clone(),
            _ => Y::default(),
        }
    }
}

impl<X, Y> FnOnce<(&X,)> for SegmentedLinearFn<X, Y>
where
    X: XCallRequirement,
    Y: YCallRequirement,
{
    type Output = Y;

    extern "rust-call" fn call_once(self, args: (&X,)) -> Self::Output {
        self._call(args.0)
    }
}

impl<X, Y> FnMut<(&X,)> for SegmentedLinearFn<X, Y>
where
    X: XCallRequirement,
    Y: YCallRequirement,
{
    extern "rust-call" fn call_mut(&mut self, args: (&X,)) -> Self::Output {
        self._call(args.0)
    }
}

impl<X, Y> Fn<(&X,)> for SegmentedLinearFn<X, Y>
where
    X: XCallRequirement,
    Y: YCallRequirement,
{
    extern "rust-call" fn call(&self, args: (&X,)) -> Self::Output {
        self._call(args.0)
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
            0 => return,
            1 => {
                let (x0, y0) = rhs.points.iter().next().unwrap();
                if self.len() == 0 {
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

    #[test]
    fn test_segmented_linear_fn() {
        let f = SegmentedLinearFn::from_iter(
            [(0., 0.), (1., 1.), (2., 1.), (3., 2.), (4., 0.)].map(|(x, y)| (f64o::from(x), y)),
        );
        assert!(f(&f64o::from(-1.)) - 0. < 1e-10);
        assert!(f(&f64o::from(0.5)) - 0.5 < 1e-10);
        assert!(f(&f64o::from(1.5)) - 1. < 1e-10);
        assert!(f(&f64o::from(2.5)) - 1.5 < 1e-10);
        assert!(f(&f64o::from(3.5)) - 1. < 1e-10);
        assert!(f(&f64o::from(5.)) - 0. < 1e-10);
    }

    #[test]
    fn test_segmented_linear_fn_add() {
        let f = SegmentedLinearFn::from_iter(
            [(0., 0.), (1., 1.), (2., 1.), (3., 2.), (4., 0.)].map(|(x, y)| (f64o::from(x), y)),
        );
        let g =
            SegmentedLinearFn::from_iter([(-1., 0.), (5., 1.)].map(|(x, y)| (f64o::from(x), y)));
        let h = f + g;
        dbg!(&h);
    }
}
