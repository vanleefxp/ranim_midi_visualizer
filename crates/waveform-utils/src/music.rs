use std::{
    collections::{BTreeMap, HashMap},
    fmt::Debug,
    ops::{Deref, DerefMut, Range},
    sync::{MappedRwLockReadGuard, RwLock, RwLockReadGuard},
};

use derivative::Derivative;
use ordered_float::OrderedFloat;
use ranim_midi_visualizer_math::func::{LadderFn, SegmentedLinearFn};
use simple_interval_tree::IntervalTree;
use smallvec::SmallVec;
use typed_floats::{NonNaNFinite, tf64};

#[allow(non_camel_case_types)]
type f64o = OrderedFloat<f64>;
type Metric = tf64::NonNaNFinite;
type Velocity = tf64::PositiveFinite;
type Tempo = tf64::PositiveFinite;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Derivative)]
#[derivative(Default)]
pub struct Note<Pitch = i8> {
    /// The pitch of the note.
    pub pitch: Pitch,
    /// The velocity of the note. Should be a value between 0 and 1.
    #[derivative(Default(value = "0.75.try_into().unwrap()"))]
    pub velocity: Velocity,
    // /// A slight offset from the standard start and end positions of the note.
    // pub offset: [Metric; 2],
}

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

impl<Pitch> Default for Voice<Pitch> {
    fn default() -> Self {
        Self {
            notes: IntervalTree::new(),
        }
    }
}

impl<Pitch> Voice<Pitch> {
    fn remap(self, time_map: &SegmentedLinearFn<f64o, f64>) -> Self {
        let timed_notes = self.notes.into_iter_by_start().map(|(range, note)| {
            let Range { start, end } = range;
            let start_time = time_map.eval(&f64o::from(f64::from(start)), true).try_into().unwrap();
            let end_time = time_map.eval(&f64o::from(f64::from(end)), true).try_into().unwrap();
            let time_range = start_time..end_time;
            (time_range, note)
        }).collect();
        Self { notes: timed_notes }
    }
}

#[derive(Debug, Clone)]
pub struct Staff<Pitch = i8, Control = PedalControl> {
    pub voices: Vec<Voice<Pitch>>,
    pub controls: BTreeMap<Metric, SmallVec<[Control; 1]>>,
}

impl<Pitch, Control> Staff<Pitch, Control> {
    fn remap(self, time_map: &SegmentedLinearFn<f64o, f64>) -> Self {
        let voices = self.voices.into_iter().map(|voice| {
            voice.remap(time_map)
        }).collect();
        let controls = self.controls.into_iter().map(|(beat, controls)| {
            let time = time_map.eval(&f64o::from(f64::from(beat)), true).try_into().unwrap();
            (time, controls)
        }).collect();
        Self {
            voices,
            controls,
        }
    }
}

impl<Pitch, Control> Default for Staff<Pitch, Control> {
    fn default() -> Self {
        Self {
            voices: Vec::new(),
            controls: BTreeMap::new(),
        }
    }
}

/// Music without metric information.
#[derive(Debug, Clone)]
pub struct RawMusic<Pitch = i8, Control = PedalControl> {
    /// Total duration of the music in beats.
    pub duration: Metric,
    /// The staves / tracks of the music.
    pub staves: Vec<Staff<Pitch, Control>>,
}

impl<Pitch, Control> Default for RawMusic<Pitch, Control> {
    fn default() -> Self {
        Self {
            duration: Default::default(),
            staves: Default::default(),
        }
    }
}

impl<Pitch, Control> From<Music<Pitch, Control>> for RawMusic<Pitch, Control> {
    // Convert music with metric-based timing to linear timing, erasing all metric information.
    fn from(value: Music<Pitch, Control>) -> Self {
        let Music { inner: RawMusic { duration, staves }, tempo, time_map } = value;
        let time_map = match time_map.into_inner().unwrap() {
            Some(v) => v,
            None => generate_time_map(&tempo),
        };
        let duration = NonNaNFinite::try_from(time_map.eval(&f64o::from(f64::from(duration)), true)).unwrap();
        let staves = staves.into_iter().map(|v| v.remap(&time_map)).collect();
        
        Self { duration, staves }
    }
}

#[derive(Debug)]
pub struct Music<Pitch = i8, Control = PedalControl> {
    inner: RawMusic<Pitch, Control>,
    /// The tempo curve of the music. Maps beats to tempo values in seconds per beat.
    pub tempo: LadderFn<Metric, Tempo>,

    // cache fields
    /// A mapping from metric beats to time in seconds.
    time_map: RwLock<Option<SegmentedLinearFn<f64o, f64>>>,
}

impl<Pitch, Control> Clone for Music<Pitch, Control>
where
    Pitch: Clone,
    Control: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            tempo: self.tempo.clone(),
            // do not clone cache fields
            time_map: RwLock::new(None),
        }
    }
}

impl<Pitch, Control> Default for Music<Pitch, Control>
where
    Metric: Default + Ord,
{
    fn default() -> Self {
        Self {
            inner: Default::default(),
            // default tempo is 60 BPM (1 second per note)
            tempo: LadderFn::from_iter([(Default::default(), 1.0.try_into().unwrap())]),
            time_map: RwLock::new(None),
        }
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
    pub fn time_map(&self) -> MappedRwLockReadGuard<'_, SegmentedLinearFn<f64o, f64>> {
        let mut time_map = self.time_map.write().unwrap();
        if time_map.is_none() {
            *time_map = Some(generate_time_map(&self.tempo));
        }
        RwLockReadGuard::map(
            self.time_map.read().unwrap(),
            |time_map: &Option<SegmentedLinearFn<f64o, f64>>| time_map.as_ref().unwrap(),
        )
    }

    pub fn time_at(&self, beat: f64) -> f64 {
        self.time_map().eval(&f64o::from(beat), true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pedal {
    Soft,
    Sostenuto,
    Sustain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PedalControl {
    /// Which pedal is being controlled.
    pub pedal: Pedal,
    /// The depth of the pedal being held. Should be a value between 0 and 1.
    pub depth: tf64::PositiveFinite,
}

fn generate_time_map(tempo: &LadderFn<Metric, Tempo>) -> SegmentedLinearFn<f64o, f64> {
    let map = tempo
        .iter()
        .scan(
            (0.0f64, 0.0f64, 1.0f64),
            |(cur_time, cur_beat, cur_tempo), (&beat, &tempo)| {
                let point = (f64o::from(*cur_time), *cur_beat);
                *cur_time += (f64::from(beat) - *cur_beat) * *cur_tempo;
                *cur_beat = beat.into();
                *cur_tempo = tempo.into();
                Some(point)
            },
        )
        .collect();
    map
}

pub fn parse_midi(src: &[u8]) -> Result<Music, midly::Error> {
    let (header, track_iter) = midly::parse(src)?;

    // TODO: handle sequential multiple tracks
    let ticks_per_beat = {
        use midly::Timing::*;
        match header.timing {
            Metrical(n) => n.as_int(),
            Timecode(fps, subframes) => {
                let fps = fps.as_int();
                fps as u16 * subframes as u16
            }
        }
    };
    let tick_to_beat = |tick: u64| {
        // SAFETY: `tick` is `u64` so it's guaranteed to be non-negative,
        // `ticks_per_beat` is guaranteed to be positive.
        tf64::NonNaNFinite::try_from(tick as f64 / ticks_per_beat as f64).unwrap()
    };

    let mut cur_tick = 0u64;
    let mut music = Music::default();

    for event_iter in track_iter {
        let event_iter = event_iter?;
        let mut staff = Staff::default();
        let mut note_states =
            HashMap::<(u8, i8), SmallVec<[(u64, Note<i8>); 1]>>::with_capacity(10);

        for event in event_iter {
            let event = event?;
            // advance time
            cur_tick += event.delta.as_int() as u64;

            use midly::TrackEventKind::*;
            match event.kind {
                Midi {
                    channel: voice,
                    message,
                } => {
                    use midly::MidiMessage::*;
                    let voice = voice.as_int();
                    match message {
                        NoteOn {
                            key: pitch,
                            vel: velocity,
                        } => {
                            // regularize pitch to middle C as 0
                            let pitch = pitch.as_int() as i8 - 60;
                            // regularize velocity to 0.0..=1.0 range
                            let velocity =
                                tf64::PositiveFinite::new(velocity.as_int() as f64 / 127.0)
                                    .unwrap();
                            // insert note into the note states table
                            let note = Note {
                                pitch,
                                velocity,
                                // offset: Default::default(),
                            };
                            note_states
                                .entry((voice, pitch))
                                .or_default()
                                .push((cur_tick, note));
                        }
                        NoteOff { key: pitch, .. } => {
                            // regularize pitch to middle C as 0
                            let pitch = pitch.as_int() as i8 - 60;

                            let should_remove =
                                if let Some(notes) = note_states.get_mut(&(voice, pitch)) {
                                    // SAFETY: `notes` must be non-empty at this point, so we can unwrap it safely.
                                    let (start_tick, note) = notes.pop().unwrap();

                                    let start_beat = tick_to_beat(start_tick);
                                    let end_beat = tick_to_beat(cur_tick);
                                    let range = start_beat..end_beat;

                                    // insert note into the voice's interval tree
                                    staff.voices[voice as usize].notes.insert(range, note);
                                    notes.is_empty()
                                } else {
                                    false
                                };

                            // remove the `SmallVec` associated with the note state if it's empty
                            if should_remove {
                                note_states.remove(&(voice, pitch));
                            }
                        }
                        Controller { controller, value } => {
                            use Pedal::*;
                            let pedal = match controller.as_int() {
                                64 => Some(Sustain),
                                66 => Some(Sostenuto),
                                67 => Some(Soft),
                                _ => None,
                            };
                            if let Some(pedal) = pedal {
                                let depth =
                                    tf64::PositiveFinite::new(value.as_int() as f64 / 127.0)
                                        .unwrap();
                                let control = PedalControl { pedal, depth };
                                staff
                                    .controls
                                    .entry(tick_to_beat(cur_tick))
                                    .or_default()
                                    .push(control);
                            }
                        }
                        _ => (),
                    }
                    staff
                        .voices
                        .resize_with(voice as usize, || Voice::default());
                }
                Meta(message) => {
                    use midly::MetaMessage::*;
                    match message {
                        Tempo(tempo) => {
                            // SAFETY: original tempo is in microseconds per quarter note
                            // now converted to seconds per beat
                            // so it must be positive
                            let tempo = tf64::PositiveFinite::try_from(tempo.as_int() as f64 / 1e6)
                                .unwrap();
                            music.tempo.insert(tick_to_beat(cur_tick), tempo);
                        }
                        EndOfTrack => {
                            music.duration = music.duration.max(tick_to_beat(cur_tick));
                        }
                        _ => (),
                    }
                }
                _ => (),
            }
        }

        music.staves.push(staff);
    }

    Ok(music)
}
