use std::{
    collections::BTreeMap,
    num::NonZeroU64,
    ops::{Range, RangeBounds},
};

use itertools::Itertools as _;
use simple_interval_tree::Endpoint;
use smallvec::SmallVec;

use crate::music::{ControlContainer, NoteContainer, TimeMap};

use super::{Metric, Note, PedalControl, Voice};

#[derive(Debug, Clone)]
pub struct Staff<Pitch = i8, Control = PedalControl> {
    pub voices: Vec<Voice<Pitch>>,
    pub(crate) controls: BTreeMap<Metric, SmallVec<[Control; 1]>>,
}

impl<Pitch, Control> Default for Staff<Pitch, Control> {
    fn default() -> Self {
        Self {
            voices: Vec::new(),
            controls: BTreeMap::new(),
        }
    }
}

impl<Pitch, Control> Staff<Pitch, Control> {
    pub(crate) fn remap(self, time_map: &TimeMap) -> Self {
        let voices = self
            .voices
            .into_iter()
            .map(|voice| voice.remap(time_map))
            .collect();
        let controls = self
            .controls
            .into_iter()
            .map(|(tick, controls)| {
                let time = time_map.eval(&tick, true);
                (time, controls)
            })
            .collect();
        Self { voices, controls }
    }

    pub fn notes_by_start_with_pos(
        &self,
    ) -> impl Iterator<Item = (&(Range<Metric>, Note<Pitch>), usize)> {
        self.voices
            .iter()
            .enumerate()
            .map(|(idx, voice)| voice.notes_by_start().map(move |v| (v, idx)))
            .kmerge_by(|((range1, _), _), ((range2, _), _)| range1.start < range2.start)
    }

    pub fn note_instants_during_with_pos<'a, G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = (Endpoint<'a, Metric, Note<Pitch>>, usize)>
    where
        G: RangeBounds<Metric> + Clone,
    {
        self.voices
            .iter()
            .enumerate()
            .map(|(idx, voice)| voice.note_instants_during(range).map(move |v| (v, idx)))
            .kmerge_by(|(ep1, _), (ep2, _)| ep1.at < ep2.at)
    }

    pub fn note_instants_with_pos<'a>(
        &'a self,
    ) -> impl Iterator<Item = (Endpoint<'a, Metric, Note<Pitch>>, usize)> {
        self.note_instants_during_with_pos(&..)
    }

    pub fn notes_during_with_pos<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = (&(Range<Metric>, Note<Pitch>), usize)>
    where
        G: RangeBounds<Metric> + Clone,
    {
        self.voices
            .iter()
            .enumerate()
            .map(|(idx, voice)| voice.notes_during(range).map(move |v| (v, idx)))
            .kmerge_by(|((range1, _), _), ((range2, _), _)| range1.start < range2.start)
    }

    pub fn notes_overlaps_with_pos<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = (&(Range<Metric>, Note<Pitch>), usize)>
    where
        G: RangeBounds<Metric> + Clone,
    {
        self.voices
            .iter()
            .enumerate()
            .map(|(idx, voice)| voice.notes_overlaps(range).map(move |v| (v, idx)))
            .kmerge_by(|((range1, _), _), ((range2, _), _)| range1.start < range2.start)
    }

    pub fn controls_during_with_pos<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = ((Metric, &Control), usize)>
    where
        G: RangeBounds<Metric> + Clone,
    {
        self.controls
            .range(range.clone())
            .enumerate()
            .flat_map(|(idx, (&beat, controls))| controls.iter().map(move |v| ((beat, v), idx)))
    }

    pub fn controls_with_pos(&self) -> impl Iterator<Item = ((Metric, &Control), usize)> {
        self.controls_during_with_pos(&..)
    }
}

impl<'a, Pitch: 'a, Control> NoteContainer<'a, Pitch> for Staff<Pitch, Control> {
    fn note_instants_during<G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = Endpoint<'a, Metric, Note<Pitch>>>
    where
        G: RangeBounds<Metric>,
    {
        self.voices
            .iter()
            .map(|voice| voice.note_instants_during(range))
            .kmerge_by(|ep1, ep2| ep1.at < ep2.at)
    }

    fn notes_by_start(&self) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)> {
        self.voices
            .iter()
            .map(|voice| voice.notes_by_start())
            .kmerge_by(|(range1, _), (range2, _)| range1.start < range2.start)
    }

    fn note_count(&self) -> usize {
        self.voices.iter().map(|voice| voice.note_count()).sum()
    }

    fn notes_during<G>(&self, range: &G) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.voices
            .iter()
            .flat_map(|voice| voice.notes_during(range))
    }

    fn notes_overlaps<G>(&self, range: &G) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.voices
            .iter()
            .flat_map(|voice| voice.notes_overlaps(range))
    }

    fn notes_starts_during<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.voices
            .iter()
            .map(|voice| voice.notes_starts_during(range))
            .kmerge_by(|(range1, _), (range2, _)| range1.start < range2.start)
    }

    fn notes_ends_during<G>(&self, range: &G) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.voices
            .iter()
            .map(|voice| voice.notes_ends_during(range))
            .kmerge_by(|(range1, _), (range2, _)| range1.start < range2.start)
    }

    fn note_rate(&self, time: u64, window: NonZeroU64) -> usize {
        self.voices.iter().map(|v| v.note_rate(time, window)).sum()
    }

    fn legato_index(&self, time: u64, window: NonZeroU64) -> f64 {
        self.voices
            .iter()
            .map(|v| v.legato_index(time, window))
            .sum::<f64>()
            .copysign(1.0) // prevent negative zero
    }
}

impl<'a, Pitch, Control: 'a> ControlContainer<'a, Control> for Staff<Pitch, Control> {
    fn controls_during<G>(&self, range: &G) -> impl Iterator<Item = (Metric, &Control)>
    where
        G: RangeBounds<Metric> + Clone,
    {
        self.controls
            .range(range.clone())
            .flat_map(|(&beat, controls)| controls.iter().map(move |v| (beat, v)))
    }
}
