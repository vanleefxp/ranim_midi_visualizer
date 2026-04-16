use derive_more::{AsMut, AsRef, Deref, DerefMut, From, Into};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Deref, DerefMut, AsRef, AsMut, Default, From, Into)]
pub struct LadderFn<X, Y> {
    points: BTreeMap<X, Y>,
}

impl<X, Y> FromIterator<(X, Y)> for LadderFn<X, Y>
where
    X: Ord,
{
    fn from_iter<T: IntoIterator<Item = (X, Y)>>(iter: T) -> Self {
        BTreeMap::from_iter(iter).into()
    }
}

trait XCallRequirement = Ord;
trait YCallRequirement = Default + Clone;

impl<X, Y> LadderFn<X, Y> {
    fn _call(&self, x: &X) -> Y
    where
        X: XCallRequirement,
        Y: YCallRequirement,
    {
        match self.len() {
            0 => Y::default(),
            _ => match self.points.range(..=x).next_back() {
                Some((_, y)) => y.clone(),
                None => self.points.iter().next().unwrap().1.clone(),
            },
        }
    }
}

impl<X, Y> FnOnce<(&X,)> for LadderFn<X, Y>
where
    X: XCallRequirement,
    Y: YCallRequirement,
{
    type Output = Y;

    extern "rust-call" fn call_once(self, args: (&X,)) -> Self::Output {
        self._call(args.0)
    }
}

impl<X, Y> FnMut<(&X,)> for LadderFn<X, Y>
where
    X: XCallRequirement,
    Y: YCallRequirement,
{
    extern "rust-call" fn call_mut(&mut self, args: (&X,)) -> Self::Output {
        self._call(args.0)
    }
}

impl<X, Y> Fn<(&X,)> for LadderFn<X, Y>
where
    X: XCallRequirement,
    Y: YCallRequirement,
{
    extern "rust-call" fn call(&self, args: (&X,)) -> Self::Output {
        self._call(args.0)
    }
}
