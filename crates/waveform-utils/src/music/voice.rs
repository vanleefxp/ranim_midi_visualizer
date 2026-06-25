use std::{
    fmt::Debug,
    ops::{Deref, Range},
};

use simple_interval_tree::IntervalTree;

use crate::music::TimeMap;

use super::{Metric, Note};

#[derive(Clone)]
pub struct Voice<Pitch = i8> {
    pub notes: IntervalTree<Metric, Note<Pitch>>,
}

impl<Pitch> Debug for Voice<Pitch>
where
    Pitch: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Voice").field("notes", &self.notes).finish()
    }
}

impl<Pitch> Deref for Voice<Pitch> {
    type Target = IntervalTree<Metric, Note<Pitch>>;

    fn deref(&self) -> &Self::Target {
        &self.notes
    }
}

impl<Pitch> Default for Voice<Pitch> {
    fn default() -> Self {
        Self {
            notes: IntervalTree::new(),
        }
    }
}

impl<Pitch> Voice<Pitch> {
    pub(crate) fn remap(self, time_map: &TimeMap) -> Self {
        let timed_notes = self
            .notes
            .into_iter_by_start()
            .map(|(tick_range, note)| {
                let Range { start, end } = tick_range;
                let start_time = time_map.eval(&start, true);
                let end_time = time_map.eval(&end, true);
                (start_time..end_time, note)
            })
            .collect();
        Self { notes: timed_notes }
    }
}
