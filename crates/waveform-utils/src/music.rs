use std::{collections::{BTreeMap, HashMap}, fmt::Debug};

use derivative::Derivative;
use simple_interval_tree::IntervalTree;
use smallvec::SmallVec;
use typed_floats::tf64;

type Velocity = tf64::PositiveFinite;
type Tempo = tf64::StrictlyPositiveFinite;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Derivative)]
#[derivative(Default)]
pub struct Note<Pitch = i8, Metric = tf64::NonNaNFinite> {
    /// The pitch of the note.
    pub pitch: Pitch,
    /// The velocity of the note. Should be a value between 0 and 1.
    #[derivative(Default(value = "0.75.try_into().unwrap()"))]
    pub velocity: Velocity,
    /// A slight offset from the standard start and end positions of the note.
    pub offset: [Metric; 2],
}

#[derive(Clone)]
pub struct Voice<Pitch = i8, Metric = tf64::NonNaNFinite> {
    pub notes: IntervalTree<Metric, Note<Pitch>>,
}

impl<Pitch, Metric> Debug for Voice<Pitch, Metric> where Pitch: Debug, Metric: Debug + Ord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Voice").field("notes", &self.notes).finish()
    }
}

impl<Pitch, Metric> Default for Voice<Pitch, Metric> {
    fn default() -> Self {
        Self {
            notes: IntervalTree::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Staff<Pitch = i8, Metric = tf64::NonNaNFinite, Control = PedalControl> {
    pub voices: Vec<Voice<Pitch>>,
    pub controls: BTreeMap<Metric, SmallVec<[Control; 1]>>,
}

impl<Pitch, Metric, Control> Default for Staff<Pitch, Metric, Control> {
    fn default() -> Self {
        Self {
            voices: Vec::new(),
            controls: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Music<Pitch = i8, Metric = tf64::NonNaNFinite, Control = PedalControl> {
    pub duration: Metric,
    pub staves: Vec<Staff<Pitch, Metric, Control>>,
    pub tempo: BTreeMap<Metric, Tempo>,
}

impl<Pitch, Metric, Control> Default for Music<Pitch, Metric, Control> where Metric: Default + Ord {
    fn default() -> Self {
        Self {
            duration: Default::default(),
            staves: Vec::new(),
            // default tempo is 60 BPM
            tempo: BTreeMap::from([(Default::default(), 1.0.try_into().unwrap())]),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

pub fn parse_midi(src: &[u8]) -> Result<Music, midly::Error> {
    let (header, track_iter) = midly::parse(src)?;

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
                            let note = Note { pitch, velocity, offset: Default::default() };
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
                            let pedal_type = match controller.as_int() {
                                64 => Some(Sustain),
                                66 => Some(Sostenuto),
                                67 => Some(Soft),
                                _ => None,
                            };
                            if let Some(pedal) = pedal_type {
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
                            // original tempo is in microseconds per quarter note
                            // now converted to seconds per beat
                            // it must be strictly positive
                            let tempo =
                                tf64::StrictlyPositiveFinite::try_from(tempo.as_int() as f64 / 1e6)
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
