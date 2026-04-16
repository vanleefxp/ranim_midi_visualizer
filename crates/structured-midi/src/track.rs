use std::{
    collections::BTreeSet,
    ops::{Bound, IntoBounds, Range, RangeBounds},
};

use interavl::{IntervalTree, Node};

use crate::GenericPedalInstant;

use super::instant::GenericMidiNoteInstant;
use super::loc::Channel;
use super::note::GenericMidiNote;

#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[non_exhaustive]
pub struct GenericMidiTrack<L>
where
    L: Ord,
{
    pub notes: IntervalTree<u64, GenericMidiNote<L>>,
    pub pedals: BTreeSet<GenericPedalInstant<L>>,
}

impl<L> GenericMidiTrack<L>
where
    L: Ord,
{
    pub fn instants(&self) -> BTreeSet<GenericMidiNoteInstant<L>>
    where
        L: Copy,
    {
        let mut instants = BTreeSet::new();
        for node in self.notes.iter() {
            let &GenericMidiNote { loc, key, vel } = node.value();
            let &Range { start, end } = node.range();
            instants.insert(GenericMidiNoteInstant::new_start(start, loc, key, vel));
            instants.insert(GenericMidiNoteInstant::new_end(end, loc, key));
        }
        instants
    }

    // [TODO] make this better
    pub fn notes_between_iter<'a>(
        &'a self,
        time_range: &'a Range<u64>,
        key_range: &impl RangeBounds<i8>,
    ) -> impl Iterator<Item = &'a Box<Node<u64, GenericMidiNote<L>>>> {
        self.notes
            .iter_overlaps(time_range)
            .filter(|node| key_range.contains(&node.value().key))
    }

    pub fn snap_pos(&self, time: u64) -> u64 {
        self.notes
            .closest_interval_start(&time)
            .cloned()
            .unwrap_or_default()
    }

    pub fn next_snap_pos(&self, time: u64) -> Option<u64> {
        self.notes.next_interval_start(&time).cloned()
    }

    pub fn prev_snap_pos(&self, time: u64) -> Option<u64> {
        self.notes.prev_interval_start(&time).cloned()
    }

    pub fn duration(&self) -> u64 {
        self.notes.max_interval_end().cloned().unwrap_or_default()
    }

    // notes per second in the given time window right before `time`
    pub fn nps(&self, time: u64, window: u64) -> f64 {
        let time_start = time.checked_sub(window).unwrap_or_default();
        let range = time_start..time;
        self.notes
            .iter_overlaps(&range)
            .filter(|entry| entry.range().start >= time_start)
            .count() as f64
            / (window as f64 / 1e9)
    }

    pub fn legato_index(&self, time: u64, window: u64) -> f64 {
        let time_start = time.checked_sub(window).unwrap_or_default();
        let range = time_start..time;
        let duration_sum: u64 = {
            use Bound::*;
            self.notes
                .iter_overlaps(&range)
                .map(|entry| entry.range().clone())
                .map(|r| {
                    let (start, end) = range.clone().intersect(r);
                    match (start, end) {
                        (Included(a) | Excluded(a), Included(b) | Excluded(b)) => b - a,
                        _ => unreachable!(),
                    }
                })
                .sum()
        };
        duration_sum as f64 / window as f64
    }
}

pub type MidiTrack = GenericMidiTrack<Channel>;
