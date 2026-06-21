use std::{collections::HashMap, num::NonZero};

use smallvec::SmallVec;
use typed_floats::tf64;

use crate::music::DEFAULT_TIME_RESOLUTION;

use super::{Metric, Music, Note, Pedal, PedalControl, RawMusic, Staff};

fn get_beat_resolution(timing: midly::Timing) -> NonZero<Metric> {
    NonZero::try_from({
        use midly::Timing::*;
        match timing {
            Metrical(n) => n.as_int() as u64,
            Timecode(fps, subframes) => {
                let fps = fps.as_int();
                fps as u64 * subframes as u64
            }
        }
    })
    .expect("Beat resolution should be strictly positive.")
}

pub fn parse_midi_raw(src: &[u8]) -> Result<RawMusic, midly::Error> {
    let time_resolution = DEFAULT_TIME_RESOLUTION; // [TODO] time_resolution as argument
    let (header, track_iter) = midly::parse(src)?;

    // TODO: handle sequential multiple tracks
    let beat_resolution = get_beat_resolution(header.timing);
    let mut music = RawMusic::new(time_resolution);

    for event_iter in track_iter {
        let event_iter = event_iter?;
        let mut cur_time = 0u64;
        let mut time_units_per_beat = time_resolution;

        let mut staff = Staff::default();
        let mut note_states =
            HashMap::<(u8, i8), SmallVec<[(u64, Note<i8>); 1]>>::with_capacity(10);

        for event in event_iter {
            let event = event?;
            // advance time
            let dt = (event.delta.as_int() as u128 * time_units_per_beat.get() as u128
                / beat_resolution.get() as u128) as u64;
            cur_time += dt;

            use midly::TrackEventKind::*;
            match event.kind {
                Midi {
                    message,
                    channel: voice,
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
                            let note = Note { pitch, velocity };
                            note_states
                                .entry((voice, pitch))
                                .or_default()
                                .push((cur_time, note));
                        }
                        NoteOff { key: pitch, .. } => {
                            // regularize pitch to middle C as 0
                            let pitch = pitch.as_int() as i8 - 60;

                            let should_remove =
                                if let Some(notes) = note_states.get_mut(&(voice, pitch)) {
                                    // SAFETY: `notes` must be non-empty at this point, so we can unwrap it safely.
                                    let (start_time, note) = notes.pop().unwrap();
                                    let voice = voice as usize;

                                    // insert note into the voice's interval tree
                                    if staff.voices.len() < voice + 1 {
                                        staff.voices.resize_with(voice + 1, Default::default);
                                    }
                                    staff.voices[voice].notes.insert(start_time..cur_time, note);
                                    notes.is_empty()
                                } else {
                                    false
                                };

                            // remove the `SmallVec` associated with the note state if it's empty
                            if should_remove {
                                note_states.remove(&(voice, pitch));
                            }
                        }
                        _ => (),
                    }
                }
                Meta(message) => {
                    use midly::MetaMessage::*;
                    match message {
                        Tempo(tempo) => {
                            time_units_per_beat = ((tempo.as_int() as f64
                                * (time_resolution.get() as f64 / 1e6))
                                as u64)
                                .try_into()
                                .expect("Tempo should be strictly positive.");
                        }
                        EndOfTrack => {
                            music.duration = music.duration.max(cur_time);
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

/// Parse a MIDI file with tempo information.
pub fn parse_midi(src: &[u8]) -> Result<Music, midly::Error> {
    let (header, track_iter) = midly::parse(src)?;

    // TODO: handle sequential multiple tracks
    let beat_resolution = get_beat_resolution(header.timing);

    let mut music = Music::from_beat_resolution(beat_resolution);

    for event_iter in track_iter {
        let event_iter = event_iter?;

        let mut cur_tick = 0u64;
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
                                    let voice = voice as usize;

                                    // insert note into the voice's interval tree
                                    if staff.voices.len() < voice + 1 {
                                        staff.voices.resize_with(voice + 1, Default::default);
                                    }
                                    staff.voices[voice].notes.insert(start_tick..cur_tick, note);
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
                                staff.controls.entry(cur_tick).or_default().push(control);
                            }
                        }
                        _ => (),
                    }
                }
                Meta(message) => {
                    use midly::MetaMessage::*;
                    match message {
                        Tempo(tempo) => {
                            // `tempo` in nanoseconds per beat
                            let tempo = NonZero::try_from(tempo.as_int() as u64 * 1000)
                                .expect("Tempo should be strictly positive.");
                            println!("{} {}", cur_tick, tempo);
                            music.tempo.insert(cur_tick, tempo);
                        }
                        EndOfTrack => {
                            music.duration = music.duration.max(cur_tick);
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

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;

    #[test]
    fn test_parse_midi() {
        let src = include_bytes!("tests/song_2.mid").as_slice();
        let music = RawMusic::from(parse_midi(src).unwrap());
        println!("{:?}", music);
    }
}
