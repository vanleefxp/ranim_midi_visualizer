use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::{Range, RangeBounds},
};

use crate::{
    MidiNoteInstant, MultiTrackLoc, MultiTrackMidiNote, MultiTrackMidiNoteInstant,
    MultiTrackPedalInstant, PedalInstant, PedalType,
    note::MidiNote,
    track::MidiTrack,
    utils::func::{LadderFn, SegmentedLinearFn},
};
use derive_more::{Deref, DerefMut, Index, IndexMut, IntoIterator};
use interavl::IntervalTree;
use itertools::Itertools as _;

use super::track::GenericMidiTrack;

#[derive(Debug, Default, Clone, IntoIterator, Deref, DerefMut, Index, IndexMut)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MidiMusic {
    tracks: Vec<MidiTrack>,
}

impl TryFrom<&[u8]> for MidiMusic {
    type Error = midly::Error;
    fn try_from(src: &[u8]) -> Result<Self, Self::Error> {
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
        let mut nanosec_per_beat = 1_000_000_000u64;
        let mut global_time = 0u64;
        let mut tracks = Vec::new();

        for event_iter in track_iter {
            let event_iter = event_iter?;

            let mut notes = IntervalTree::default();
            let mut pedals = BTreeSet::new();
            let mut note_states = HashMap::<(u8, i8), (u64, MidiNote)>::with_capacity(10);

            for event in event_iter {
                let event = event?;

                // update global time
                let dt = event.delta.as_int() as u64 * nanosec_per_beat / ticks_per_beat as u64;
                global_time += dt;

                use midly::TrackEventKind::*;
                match event.kind {
                    Midi { message, channel } => {
                        use midly::MidiMessage::*;
                        let channel = channel.as_int();
                        match message {
                            NoteOn { key, vel } => {
                                let key = key.as_int() as i8 - 60;
                                let vel = vel.as_int();
                                if let Some((start_time, note)) =
                                    note_states.get_mut(&(channel, key))
                                {
                                    // overlapping note
                                    // cut off current note and restart it
                                    notes.insert(*start_time..global_time, *note);
                                    *note = MidiNote {
                                        loc: channel,
                                        key,
                                        vel,
                                    };
                                } else {
                                    note_states.insert(
                                        (channel, key),
                                        (
                                            global_time,
                                            MidiNote {
                                                loc: channel,
                                                key,
                                                vel,
                                            },
                                        ),
                                    );
                                }
                            }
                            NoteOff { key, .. } => {
                                let key = key.as_int() as i8 - 60;
                                if let Some((start_time, note)) =
                                    note_states.remove(&(channel, key))
                                {
                                    // stop of note
                                    notes.insert(start_time..global_time, note);
                                }
                            }
                            Controller { controller, value } => {
                                use PedalType::*;
                                let pedal_type = match controller.as_int() {
                                    64 => Some(Sustain),
                                    66 => Some(Sostenuto),
                                    67 => Some(Soft),
                                    _ => None,
                                };
                                if let Some(pedal_type) = pedal_type {
                                    let instant = PedalInstant {
                                        time: global_time,
                                        loc: channel,
                                        pedal_type,
                                        value: value.as_int(),
                                    };
                                    pedals.insert(instant);
                                }
                            }
                            _ => {}
                        }
                    }
                    Meta(meta_msg) => {
                        use midly::MetaMessage::*;
                        match meta_msg {
                            Tempo(value) => {
                                // tempo change
                                nanosec_per_beat = value.as_int() as u64 * 1000;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            tracks.push(GenericMidiTrack { notes, pedals });
        }
        Ok(Self { tracks })
    }
}

impl MidiMusic {
    pub fn instants(&self) -> impl Iterator<Item = MultiTrackMidiNoteInstant> {
        self.tracks
            .iter()
            .enumerate()
            .map(|(idx, track)| {
                track
                    .instants()
                    .into_iter()
                    .map(move |instant| (idx, instant))
            })
            .kmerge_by(|(_, a), (_, b)| a.time < b.time)
            .map(|(idx, instant)| {
                let MidiNoteInstant {
                    time,
                    loc: channel,
                    key,
                    vel,
                } = instant;
                let loc = MultiTrackLoc {
                    track: idx,
                    channel,
                };
                MultiTrackMidiNoteInstant {
                    time,
                    loc,
                    key,
                    vel,
                }
            })
    }

    pub fn notes(&self) -> impl Iterator<Item = (Range<u64>, MultiTrackMidiNote)> {
        self.tracks
            .iter()
            .enumerate()
            .map(|(idx, track)| track.notes.iter().map(move |v| (idx, v)))
            .kmerge_by(|(_, a), (_, b)| a.range().start < b.range().start)
            .map(|(idx, v)| {
                let range = v.range().clone();
                let &MidiNote {
                    loc: channel,
                    key,
                    vel,
                } = v.value();
                let loc = MultiTrackLoc {
                    track: idx,
                    channel,
                };
                let note = MultiTrackMidiNote { loc, key, vel };
                (range, note)
            })
    }

    // [TODO] make this better
    pub fn notes_between_iter<'a>(
        &'a self,
        time_range: &'a Range<u64>,
        key_range: &impl RangeBounds<i8>,
    ) -> impl Iterator<Item = (Range<u64>, MultiTrackMidiNote)> {
        self.tracks
            .iter()
            .enumerate()
            .map(|(idx, track)| {
                track
                    .notes_between_iter(time_range, key_range)
                    .map(move |v| (idx, v))
            })
            .kmerge_by(|(_, a), (_, b)| a.range().start < b.range().start)
            .map(|(idx, v)| {
                let range = v.range().clone();
                let &MidiNote {
                    loc: channel,
                    key,
                    vel,
                } = v.value();
                let loc = MultiTrackLoc {
                    track: idx,
                    channel,
                };
                let note = MultiTrackMidiNote { loc, key, vel };
                (range, note)
            })
    }

    pub fn pedals(&self) -> impl Iterator<Item = MultiTrackPedalInstant> {
        self.tracks
            .iter()
            .enumerate()
            .map(|(idx, track)| track.pedals.iter().map(move |instant| (idx, instant)))
            .kmerge_by(|(_, a), (_, b)| a.time < b.time)
            .map(|(idx, instant)| {
                let &PedalInstant {
                    time,
                    loc: channel,
                    pedal_type,
                    value,
                } = instant;
                let loc = MultiTrackLoc {
                    track: idx,
                    channel,
                };
                MultiTrackPedalInstant {
                    time,
                    loc,
                    pedal_type,
                    value,
                }
            })
    }

    pub fn snap_pos(&self, time: u64) -> u64 {
        self.iter()
            .map(|track| track.snap_pos(time))
            .min_by_key(|&snap_pos| snap_pos.abs_diff(time))
            .unwrap_or_default()
    }

    pub fn next_snap_pos(&self, time: u64) -> Option<u64> {
        self.iter()
            .map(|track| track.next_snap_pos(time))
            .flatten()
            .min()
    }

    pub fn prev_snap_pos(&self, time: u64) -> Option<u64> {
        self.iter()
            .map(|track| track.prev_snap_pos(time))
            .flatten()
            .max()
    }

    pub fn duration(&self) -> u64 {
        self.iter()
            .map(|track| track.duration())
            .max()
            .unwrap_or_default()
    }

    pub fn nps(&self, time: u64, window: u64) -> f64 {
        self.tracks
            .iter()
            .map(|track| track.nps(time, window))
            .sum::<f64>()
            .copysign(1.) // prevent negative zero
    }

    /// **Legato index** is a measure describing how continuously a series of notes are played.
    /// This index was put forward by Wiwi Kuan in his Pianometer program.
    /// See: https://nicechord.com/pianometer/
    ///
    /// The calculation of legato index in a certain time window is done as follows:
    ///
    /// + take the intersection of the time window and note ranges
    /// + sum the lengths of the intersecting parts of the notes and the time window
    /// + divide the sum by the length of the time window
    ///
    pub fn legato_index(&self, time: u64, window: u64) -> f64 {
        self.tracks
            .iter()
            .map(|track| track.legato_index(time, window))
            .sum::<f64>()
            .copysign(1.) // prevent negative zero
    }

    /// Calculates the legato index of the whole song. The returned result is a callable function.
    pub fn legato_fn(&self, window: u64) -> SegmentedLinearFn<u64, f64> {
        // `legato_index` calculate the legato index directly by definition,
        // However, for the computation of legato index of the whole song, this approach can be optimized given the
        // observation that the changing of legato index is a segmented linear function to time.
        //
        // the legato score function is _additive_, meaning that we can sum the legato score functions of each note
        // to get the total legato score function of the song.
        // So the first step is to create the legato score function for each note.
        self.notes()
            .map(|(range, _)| {
                // When it comes to the calculation of single-note legato score function, there are two cases:
                let Range { start, end } = range;
                let duration = end - start;
                SegmentedLinearFn::from_iter(if duration > window {
                    // Case 1: the note is longer than the time window
                    //
                    //                  =========                     window
                    //                           -----------------    t = start             legato = 0
                    //                  -----------------             t = start + window    legato = 1
                    //          -----------------                     t = end               legato = 1
                    // -----------------                              t = end + window      legato = 0
                    //
                    [
                        (start, 0.),
                        (start + window, 1.),
                        (end, 1.),
                        (end + window, 0.),
                    ]
                } else {
                    // Case 2: the note is shorter than the time window
                    //
                    //                  ========                      window
                    //                          -----                 t = start             legato = 0
                    //                     -----                      t = end               legato = duration / window
                    //                  -----                         t = start + window    legato = duration / window
                    //             -----                              t = end + window      legato = 0
                    //
                    let max_value = duration as f64 / window as f64;
                    [
                        (start, 0.),
                        (end, max_value),
                        (start + duration, max_value),
                        (end + window, 0.),
                    ]
                })
            })
            .sum()
    }

    pub fn note_count_iter(&self) -> impl Iterator<Item = (u64, usize)> {
        self.notes().scan(0usize, |count, (range, _)| {
            *count += 1;
            Some((range.start, *count))
        })
    }

    pub fn note_count_fn(&self) -> LadderFn<u64, usize> {
        self.note_count_iter().collect()
    }

    pub fn nps_iter(&self, window: u64) -> impl Iterator<Item = (u64, f64)> {
        // instants where the start of notes enter or exit the time window
        // and how many notes flows in or out at the instant
        // NPS value only changes at these instants
        let mut nps_changes: BTreeMap<u64, isize> = BTreeMap::new();
        for (range, _) in self.notes() {
            let enter_time = range.start;
            let exit_time = range.start + window;
            nps_changes
                .entry(enter_time)
                .and_modify(|cnt| *cnt += 1)
                .or_insert(1);
            nps_changes
                .entry(exit_time)
                .and_modify(|cnt| *cnt -= 1)
                .or_insert(-1);
        }

        // accumulate the number of notes in window and divide it by the window length to get NPS values
        nps_changes
            .into_iter()
            .scan(0usize, move |n_in_window, (time, n_enter)| {
                if n_enter > 0 {
                    *n_in_window += n_enter as usize;
                } else {
                    *n_in_window -= (-n_enter) as usize;
                }
                Some((time, *n_in_window as f64 / (window as f64 / 1e9)))
            })
    }

    pub fn nps_fn(&self, window: u64) -> LadderFn<u64, f64> {
        self.nps_iter(window).collect()
    }

    pub fn nps_max_iter(&self, window: u64) -> impl Iterator<Item = (u64, f64)> {
        self.nps_iter(window)
            .scan(0., |nps_max, (time, nps)| {
                if nps > *nps_max {
                    *nps_max = nps;
                    Some(Some((time, nps)))
                } else {
                    Some(None)
                }
            })
            .flatten()
    }

    pub fn nps_max_fn(&self, window: u64) -> LadderFn<u64, f64> {
        self.nps_max_iter(window).collect()
    }
}

pub type MergedMidiMusic = GenericMidiTrack<MultiTrackLoc>;

impl From<MidiMusic> for MergedMidiMusic {
    fn from(value: MidiMusic) -> Self {
        let mut notes = IntervalTree::new();
        let mut pedals = BTreeSet::new();
        for (track_idx, track) in value.tracks.iter().enumerate() {
            notes.extend(track.notes.iter().map(|node| {
                let note = node.value();
                let interval = node.interval().clone();
                let loc = MultiTrackLoc {
                    track: track_idx,
                    channel: note.loc,
                };
                (
                    interval,
                    MultiTrackMidiNote {
                        loc,
                        key: note.key,
                        vel: note.vel,
                    },
                )
            }));
            pedals.extend(track.pedals.iter().map(|instant| {
                let &PedalInstant {
                    time,
                    loc,
                    pedal_type,
                    value,
                } = instant;
                let loc = MultiTrackLoc {
                    track: track_idx,
                    channel: loc,
                };
                MultiTrackPedalInstant {
                    time,
                    loc,
                    pedal_type,
                    value,
                }
            }));
        }
        MergedMidiMusic { notes, pedals }
    }
}

impl From<MergedMidiMusic> for MidiMusic {
    fn from(value: MergedMidiMusic) -> Self {
        let mut tracks = Vec::with_capacity(10);
        for entry in value.notes.iter() {
            let note = entry.value();
            let track_idx = note.loc.track;
            if track_idx >= tracks.len() {
                tracks.resize_with(track_idx + 1, MidiTrack::default);
            }
            let track = &mut tracks[track_idx];
            track.notes.insert(entry.range().clone(), (*note).into());
        }
        for instant in value.pedals.iter() {
            let &MultiTrackPedalInstant {
                time,
                loc,
                pedal_type,
                value,
            } = instant;
            let track_idx = loc.track;
            if track_idx >= tracks.len() {
                tracks.resize_with(track_idx + 1, MidiTrack::default);
            }
            let track = &mut tracks[track_idx];
            track.pedals.insert(PedalInstant {
                time,
                loc: loc.channel,
                pedal_type,
                value,
            });
        }
        Self { tracks }
    }
}
