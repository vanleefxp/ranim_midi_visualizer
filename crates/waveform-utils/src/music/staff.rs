use std::{num::NonZeroU64, ops::Range};

use itertools::Itertools as _;
use simple_interval_tree::MultiValueBTreeMap;

use crate::music::{ControlContainer, MappedNoteControlContainer, NoteContainer, NoteInstant, TimeMap};

use super::{Metric, MetricRange, Note, PedalControl, Voice};

#[derive(Debug, Clone)]
pub struct Staff<Pitch = i8, Control = PedalControl> {
    pub voices: Vec<Voice<Pitch>>,
    pub(crate) controls: MultiValueBTreeMap<Metric, Control>,
}

impl<Pitch, Control> Default for Staff<Pitch, Control> {
    fn default() -> Self {
        Self {
            voices: Vec::new(),
            controls: MultiValueBTreeMap::new(),
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
}

macro with_pos($voices:expr,$cl:expr) {
    $voices
        .iter()
        .enumerate()
        .map(|(idx, voice)| $cl(voice).map(move |(_, range, note)| (idx, range, note)))
        .kmerge_by(|(_, range1, _), (_, range2, _)| range1.start < range2.start)
}

impl<Pitch, Control> NoteContainer for Staff<Pitch, Control> {
    type Pitch = Pitch;
    type Pos = usize;

    fn notes_by_start<'a>(
        &'a self,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &'a Note<Self::Pitch>)> {
        with_pos!(self.voices, |voice: &'a Voice<Self::Pitch>| voice
            .notes_by_start())
    }

    fn note_instants_during<'a, R>(
        &'a self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, NoteInstant<'a, Self::Pitch>)>
    where
        R: MetricRange,
    {
        self.voices
            .iter()
            .enumerate()
            .map(|(idx, voice)| {
                voice
                    .note_instants_during(range.clone())
                    .map(move |(_, v)| (idx, v))
            })
            .kmerge_by(|(_, ep1), (_, ep2)| ep1.at < ep2.at)
    }

    fn notes_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (usize, Range<Metric>, &Note<Pitch>)>
    where
        R: MetricRange,
    {
        self.voices
            .iter()
            .enumerate()
            .map(|(idx, voice)| {
                voice
                    .notes_during(range.clone())
                    .map(move |(_, range, note)| (idx, range, note))
            })
            .kmerge_by(|(_, range1, _), (_, range2, _)| range1.start < range2.start)
    }

    fn notes_overlaps<'a, R>(
        &'a self,
        range: R,
    ) -> impl Iterator<Item = (usize, Range<Metric>, &'a Note<Pitch>)>
    where
        R: MetricRange,
    {
        with_pos!(self.voices, |voice: &'a Voice<Self::Pitch>| voice
            .notes_overlaps(range.clone()))
    }

    fn notes_start_during<'a, R>(
        &'a self,
        range: R,
    ) -> impl Iterator<Item = (usize, Range<Metric>, &'a Note<Pitch>)>
    where
        R: MetricRange,
    {
        with_pos!(self.voices, |voice: &'a Voice<Self::Pitch>| voice
            .notes_start_during(range.clone()))
    }

    fn notes_end_during<'a, R>(
        &'a self,
        range: R,
    ) -> impl Iterator<Item = (usize, Range<Metric>, &'a Note<Pitch>)>
    where
        R: MetricRange,
    {
        with_pos!(self.voices, |voice: &'a Voice<Self::Pitch>| voice
            .notes_end_during(range.clone()))
    }

    fn note_count(&self) -> usize {
        self.voices.iter().map(|voice| voice.note_count()).sum()
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

impl<Pitch, Control> ControlContainer for Staff<Pitch, Control> {
    type Control = Control;

    fn controls_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Metric, &Self::Control)>
    where
        R: MetricRange,
    {
        self.controls.controls_during(range)
    }
}

pub type MappedStaff<'a, Pitch, Control, TimeMapRef> = MappedNoteControlContainer<'a, Staff<Pitch, Control>, TimeMapRef>;