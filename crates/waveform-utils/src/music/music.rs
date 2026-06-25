use std::{
    num::NonZero,
    ops::{Deref, DerefMut},
    sync::{MappedRwLockReadGuard, RwLock, RwLockReadGuard},
};

use derivative::Derivative;
use ranim_midi_visualizer_math::func::{LadderFn, SegmentedLinearFn};

use crate::music::{
    DEFAULT_BEAT_RESOLUTION, Metric, RawMusic, Tempo, control::PedalControl,
    time_map::generate_time_map,
};

/// Default number of time divisions per second.
// SAFETY: non zero literal
pub const DEFAULT_TIME_RESOLUTION: NonZero<Metric> = NonZero::new(1_000_000_000).unwrap(); // nanosecond-level resolution

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Music<Pitch = i8, Control = PedalControl> {
    pub(crate) inner: RawMusic<Pitch, Control>,
    /// Number of divisions per second
    pub time_resolution: NonZero<Metric>,
    /// The tempo curve of the music. Maps beats to tempo values in seconds per beat.
    pub tempo: LadderFn<Metric, Tempo>,

    // cache fields
    /// A mapping from metric beats to time in seconds.
    #[derivative(Debug = "ignore")]
    pub(crate) time_map: RwLock<Option<SegmentedLinearFn<Metric, Metric>>>,
}

impl<Pitch, Control> Clone for Music<Pitch, Control>
where
    Pitch: Clone,
    Control: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            time_resolution: self.time_resolution,
            tempo: self.tempo.clone(),
            // do not clone cache fields
            time_map: RwLock::new(None),
        }
    }
}

impl<Pitch, Control> Default for Music<Pitch, Control> {
    fn default() -> Self {
        Self::new(DEFAULT_BEAT_RESOLUTION, DEFAULT_TIME_RESOLUTION)
    }
}

impl<Pitch, Control> Deref for Music<Pitch, Control> {
    type Target = RawMusic<Pitch, Control>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Pitch, Control> DerefMut for Music<Pitch, Control> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<Pitch, Control> Music<Pitch, Control> {
    pub fn from_beat_resolution(beat_resolution: NonZero<Metric>) -> Self {
        Self::new(beat_resolution, DEFAULT_TIME_RESOLUTION)
    }

    pub fn new(beat_resolution: NonZero<Metric>, time_resolution: NonZero<Metric>) -> Self {
        Self {
            inner: RawMusic::new(beat_resolution),
            time_resolution,
            tempo: LadderFn::from_iter([(0, time_resolution)]),
            time_map: RwLock::new(None),
        }
    }

    pub fn time_map(&self) -> MappedRwLockReadGuard<'_, SegmentedLinearFn<Metric, Metric>> {
        let mut time_map = self.time_map.write().unwrap();
        if time_map.is_none() {
            *time_map = Some(generate_time_map(
                &self.tempo,
                self.inner.duration,
                self.resolution,
                self.time_resolution,
                false,
            ));
        }
        RwLockReadGuard::map(
            self.time_map.read().unwrap(),
            |time_map: &Option<SegmentedLinearFn<Metric, Metric>>| {
                time_map.as_ref().expect("Value should already be present.")
            },
        )
    }
}

// [FIXME] result not consistent with `parse_midi_raw`
impl<Pitch, Control> From<Music<Pitch, Control>> for RawMusic<Pitch, Control> {
    // Convert music with metric-based timing to linear timing, erasing all metric information.
    fn from(value: Music<Pitch, Control>) -> Self {
        let Music {
            inner:
                RawMusic {
                    duration,
                    resolution: beat_resolution,
                    staves,
                },
            time_resolution,
            tempo,
            time_map,
        } = value;
        let time_map = match time_map.into_inner().unwrap() {
            Some(v) => v,
            None => generate_time_map(&tempo, duration, beat_resolution, time_resolution, false),
        };
        let staves = staves.into_iter().map(|v| v.remap(&time_map)).collect();
        let duration = time_map.eval(&duration, true);
        Self {
            duration,
            resolution: time_resolution,
            staves,
        }
    }
}
