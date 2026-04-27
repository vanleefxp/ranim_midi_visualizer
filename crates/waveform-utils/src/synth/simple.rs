use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::Arc,
};

use cpal::{FromSample, Sample, SizedSample};
use derivative::Derivative;

use super::Synth;
use crate::{
    envelope::{Envelope, ExpDecay},
    freq::ToFrequency,
    synth::{MusicDirective, NoteDirective},
    waveform::{Waveform, Triangle},
};

#[derive(Clone)]
struct NoteState {
    is_on: bool,
    trigger_time: f64,
    volume: f64,
    // The note's waveform
    waveform: Arc<dyn Waveform + Send + Sync>,
    // The note's envelope
    envelope: Arc<dyn Envelope + Send + Sync>,
}

/// A simple waveform synthesizer.
#[derive(Derivative)]
#[derivative(Clone, Default)]
pub struct SimpleWaveformSynth<Note = i8> {
    /// The currently active waveform. Will be used for the next triggered note.
    #[derivative(Default(value = "Arc::new(Triangle)"))]
    pub waveform: Arc<dyn Waveform + Send + Sync>,
    /// The currently active envelope. Will be used for the next triggered note.
    #[derivative(Default(value = "Arc::new(ExpDecay(1.))"))]
    pub envelope: Arc<dyn Envelope + Send + Sync>,
    /// The maximum volume of a single note.
    /// This allows multiple notes to play simultaneously without overflowing the audio device's maximum volume.
    #[derivative(Default(value = "0.125"))]
    pub note_max_volume: f64,
    /// Currently sounding notes.
    note_states: HashMap<Note, NoteState>,
    sustain_notes: HashSet<Note>,
    sostenuto_notes: HashSet<Note>,
    /// Time (in seconds) since the first note of the currently sounding notes has been triggered.
    paused: bool,
    sustain_on: bool,
    sostenuto_on: bool,
    time: f64,
}

impl<Note> SimpleWaveformSynth<Note>
where
    Note: ToFrequency + Hash + Eq + Send + Sync + Clone,
{
    /// Trigger a note with the given volume.
    pub fn attack(&mut self, note: Note, volume: f64) {
        if self.sustain_on {
            self.sustain_notes.insert(note.clone());
        }
        self.note_states.insert(
            note,
            NoteState {
                is_on: true,
                trigger_time: self.time,
                volume,
                waveform: self.waveform.clone(),
                envelope: self.envelope.clone(),
            },
        );
    }

    /// Release a note. Different from [`Synthesizer::stop`], the note may still last for a while and gradually fade
    /// out.
    pub fn release(&mut self, note: &Note) {
        if !(self.sustain_notes.contains(note))
            && let Some(note_state) = self.note_states.get_mut(note)
            && note_state.is_on
        {
            note_state.is_on = false;
            note_state.trigger_time = self.time;
        }
    }

    /// Stop a note immediately so that it makes no sound since the current moment.
    pub fn stop_note(&mut self, note: &Note) {
        self.note_states.remove(note);
        if self.note_states.is_empty() {
            self.time = 0.0;
        }
    }

    /// Stop all notes.
    pub fn stop(&mut self) {
        self.note_states.clear();
        self.time = 0.0;
    }

    /// Pause the playback. All currently sounding notes will continue to sound when [`Synth::play`] is called.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume the playback. All notes sounding when [`Synth::pause`] was called will continue to sound.
    fn play(&mut self) {
        self.paused = false;
    }
}

impl<Note: ToFrequency> SimpleWaveformSynth<Note> {
    fn calc(&self, t: f64) -> f64 {
        let mut val = 0.0;
        for (tone, note_state) in &self.note_states {
            let t = t - note_state.trigger_time;
            let envelope_val = if note_state.is_on {
                note_state.envelope.on_attack(t)
            } else {
                note_state.envelope.on_release(t)
            };
            let freq = tone.to_frequency();
            let waveform_val = note_state.waveform.eval_cyc(t * freq);
            val += note_state.volume * envelope_val * waveform_val;
        }
        val * self.note_max_volume
    }

    pub fn active_notes<'a>(&'a self) -> impl Iterator<Item = &'a Note>
    where
        Note: 'a,
    {
        self.note_states.keys()
    }

    pub fn start_sustain(&mut self)
    where
        Note: Clone + Hash + Eq,
    {
        self.sustain_notes.extend(
            self.note_states
                .iter()
                .filter_map(|(note, state)| if state.is_on { Some(note) } else { None })
                .cloned(),
        );
        self.sustain_on = true;
    }

    pub fn stop_sustain(&mut self)
    where
        Note: Hash + Eq,
    {
        for note in self.sustain_notes.drain() {
            if (!self.sostenuto_on || !self.sostenuto_notes.contains(&note))
                && let Some(note_state) = self.note_states.get_mut(&note)
                && note_state.is_on
            {
                note_state.is_on = false;
                note_state.trigger_time = self.time;
            }
        }
        self.sustain_on = false;
    }

    pub fn start_sostenuto(&mut self)
    where
        Note: Clone + Hash + Eq,
    {
        self.sustain_notes.extend(
            self.note_states
                .iter()
                .filter_map(|(note, state)| if state.is_on { Some(note) } else { None })
                .cloned(),
        );
        self.sostenuto_on = true;
    }

    pub fn stop_sostenuto(&mut self)
    where
        Note: Hash + Eq,
    {
        for note in self.sostenuto_notes.drain() {
            if (!self.sostenuto_on || !self.sustain_notes.contains(&note))
                && let Some(note_state) = self.note_states.get_mut(&note)
                && note_state.is_on
            {
                note_state.is_on = false;
                note_state.trigger_time = self.time;
            }
        }
        self.sostenuto_on = false;
    }
}

impl<Note, Sample> Synth<MusicDirective<Note>, Sample> for SimpleWaveformSynth<Note>
where
    Note: ToFrequency + Hash + Eq + Send + Sync + Clone + 'static,
    Sample: SizedSample + FromSample<f64>,
{
    fn directive(&mut self, directive: MusicDirective<Note>) {
        use MusicDirective::*;
        match directive {
            Note(NoteDirective { pitch, volume }) => {
                if volume > 0. {
                    self.attack(pitch, volume);
                } else {
                    self.release(&pitch);
                }
            }
            PlayPause(is_play) => {
                if is_play {
                    self.play();
                } else {
                    self.pause();
                }
            }
            Stop => self.stop(),
            // _ => (),
        }
    }

    fn write_to_buffer(&mut self, config: &cpal::StreamConfig, buffer: &mut [Sample]) {
        if self.paused {
            buffer.fill(0.0f64.to_sample());
        } else {
            let n_channels = config.channels as usize;
            let sample_rate = config.sample_rate;
            let sample_count = buffer.len() / n_channels;

            for (i, value) in (0..sample_count)
                .map(|v| v as f64 / sample_rate as f64 + self.time)
                .map(|t| self.calc(t))
                .enumerate()
            {
                buffer[(i * n_channels)..((i + 1) * n_channels)].fill(value.to_sample());
            }

            // delete notes that has already stopped
            self.note_states.retain(|_, note_state| {
                let &mut NoteState {
                    is_on,
                    trigger_time,
                    ..
                } = note_state;
                is_on || trigger_time + note_state.envelope.release_time() > self.time
            });

            // reset time if no note is currently playing
            if self.note_states.is_empty() {
                self.time = 0.0;
            } else {
                let time = sample_count as f64 / sample_rate as f64;
                self.time += time;
            }
        }
    }
}
