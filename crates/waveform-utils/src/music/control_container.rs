use std::ops::RangeBounds;

use super::Metric;

pub trait ControlContainer<'a, Control: 'a> {
    fn controls_during<G>(&'a self, range: &G) -> impl Iterator<Item = (Metric, &'a Control)>
    where
        G: RangeBounds<Metric> + Clone;

    fn controls(&'a self) -> impl Iterator<Item = (Metric, &'a Control)> {
        self.controls_during(&..)
    }
}
