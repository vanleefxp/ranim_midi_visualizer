mod simple_waveform;
pub use simple_waveform::*;

/// A trait for a synthesizer that can play notes and write the resulting sound to a buffer.
pub trait Synth<Note = i8, Sample = f32>: Send + Sync
where
    Sample: cpal::SizedSample + cpal::FromSample<f64>,
{
    /// Trigger a note with the given volume.
    fn attack(&mut self, note: Note, volume: f64);
    /// Release a note. Different from [`Synthesizer::stop`], the note may still last for a while and gradually fade
    /// out.
    fn release(&mut self, note: &Note) {
        self.stop_note(note);
    }
    /// Stop a note immediately so that it makes no sound since the current moment.
    fn stop_note(&mut self, note: &Note);
    /// Stop all notes.
    fn stop(&mut self);
    fn play(&mut self);
    fn pause(&mut self);
    fn write_to_buffer(&mut self, config: &cpal::StreamConfig, buffer: &mut [Sample]);
}
