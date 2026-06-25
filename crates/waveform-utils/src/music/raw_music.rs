use std::{
    num::{NonZero, NonZeroU64},
    ops::{Range, RangeBounds},
};

use itertools::Itertools as _;

use crate::music::{
    ControlContainer, Metric, Note, NoteContainer, NoteInstant, PedalControl, Staff,
};

/// Beat resolution in divisions per time unit.
pub const DEFAULT_BEAT_RESOLUTION: NonZero<Metric> = NonZero::new(480).unwrap();

/// Music without metric information.
#[derive(Debug, Clone)]
pub struct RawMusic<Pitch = i8, Control = PedalControl> {
    /// Total duration of the music in beats.
    pub duration: Metric,
    /// The staves / tracks of the music.
    pub staves: Vec<Staff<Pitch, Control>>,
    /// Number of divisions per second.
    pub resolution: NonZero<Metric>,
}

impl<Pitch, Control> Default for RawMusic<Pitch, Control> {
    fn default() -> Self {
        Self::new(DEFAULT_BEAT_RESOLUTION)
    }
}

macro with_pos($staves:expr,$cl:expr) {
    $staves
        .iter()
        .enumerate()
        .map(|(staff_idx, staff)| {
            $cl(staff).map(move |(voice_idx, range, note)| ([staff_idx, voice_idx], range, note))
        })
        .kmerge_by(|(_, range1, _), (_, range2, _)| range1.start < range2.start)
}

impl<Pitch, Control> NoteContainer for RawMusic<Pitch, Control> {
    type Pitch = Pitch;
    type Pos = [usize; 2];

    fn notes_by_start<'a>(
        &'a self,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &'a Note<Self::Pitch>)> {
        with_pos!(self.staves, |staff: &'a Staff<Pitch, Control>| staff
            .notes_by_start())
    }

    fn notes_during<'a, G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &'a Note<Self::Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        with_pos!(self.staves, |staff: &'a Staff<Pitch, Control>| staff
            .notes_during(range))
    }
    fn notes_start_during<'a, G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &'a Note<Self::Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        with_pos!(self.staves, |staff: &'a Staff<Pitch, Control>| staff
            .notes_start_during(range))
    }

    fn notes_end_during<'a, G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &'a Note<Self::Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        with_pos!(self.staves, |staff: &'a Staff<Pitch, Control>| staff
            .notes_end_during(range))
    }

    fn notes_overlaps<'a, G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &'a Note<Self::Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        with_pos!(self.staves, |staff: &'a Staff<Pitch, Control>| staff
            .notes_overlaps(range))
    }

    fn note_instants_during<'a, G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = (Self::Pos, NoteInstant<'a, Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.staves
            .iter()
            .enumerate()
            .map(|(staff_idx, voice)| {
                voice
                    .note_instants_during(range)
                    .map(move |(voice_idx, instant)| ([staff_idx, voice_idx], instant))
            })
            .kmerge_by(|(_, instant1), (_, instant2)| instant1.at < instant2.at)
    }

    fn note_rate(&self, time: u64, window: NonZeroU64) -> usize {
        self.staves.iter().map(|v| v.note_rate(time, window)).sum()
    }

    fn legato_index(&self, time: u64, window: NonZeroU64) -> f64 {
        self.staves
            .iter()
            .map(|v| v.legato_index(time, window))
            .sum::<f64>()
            .copysign(1.0) // prevent negative zero
    }
}

impl<Pitch, Control> RawMusic<Pitch, Control> {
    pub fn new(resolution: NonZero<Metric>) -> Self {
        Self {
            duration: Default::default(),
            staves: Default::default(),
            resolution,
        }
    }
}

impl<Pitch, Control> ControlContainer for RawMusic<Pitch, Control> {
    type Control = Control;
    type Pos = usize;

    fn controls_during_with_pos<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = (usize, Metric, &Control)>
    where
        G: RangeBounds<Metric>,
    {
        self.staves
            .iter()
            .enumerate()
            .map(|(staff_idx, staff)| {
                staff
                    .controls_during(range)
                    .map(move |(time, control)| (staff_idx, time, control))
            })
            .kmerge_by(|(_, time1, _), (_, time2, _)| time1 < time2)
    }
}
