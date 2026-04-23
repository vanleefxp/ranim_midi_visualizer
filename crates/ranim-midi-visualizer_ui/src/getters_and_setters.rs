use std::{
    cell::Ref,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use cpal::traits::{DeviceTrait as _, StreamTrait};
use ranim::{Output, cmd::preview::Resolution};
use ranim_midi_visualizer_math::func::LadderFn;
use structured_midi::MidiMusic;
use tracing::{error, info};

use crate::{
    AUDIO_DEVICES, MidiVisualizerApp, MidiVisualizerAppInner, MidiVisualizerAppInner2,
    utils::MidiVisualizerConfig,
};

// Getters

impl MidiVisualizerAppInner2 {
    #[inline(always)]
    pub(crate) fn is_playing(&self) -> bool {
        self.play_start_t.is_some()
    }

    #[inline(always)]
    pub(crate) fn is_exporting(&self) -> bool {
        self.export_progress_rx.is_some()
    }

    #[inline(always)]
    pub(crate) fn resolution(&self) -> Resolution {
        Resolution {
            width: self.export_config.width,
            height: self.export_config.height,
        }
    }

    pub(crate) fn audio_device(&self) -> Option<&cpal::Device> {
        AUDIO_DEVICES.get(self.audio_device_idx as usize)
    }
}

impl MidiVisualizerAppInner {
    /// Precomputed maximum note-per-second (NPS) function for the entire music.
    /// Used to accelerate the computation of maximum NPS at a specific time.
    fn nps_max_fn(&self) -> Ref<'_, LadderFn<u64, f64>> {
        if self.cache.nps_max.borrow().is_none() {
            let nps_max_fn = self.music.nps_max_fn(self.time_window);
            self.cache.nps_max.replace(Some(nps_max_fn));
        }
        Ref::map(self.cache.nps_max.borrow(), |x| {
            x.as_ref().expect("`nps_max_fn` can't be `None`")
        })
    }

    /// Pre-computed note count function for the entire music.
    /// Used to accelerate the computation of the note count at a specific time.
    fn note_count_fn(&self) -> Ref<'_, LadderFn<u64, usize>> {
        if self.cache.note_count.borrow().is_none() {
            let note_count_fn = self.music.note_count_fn();
            self.cache.note_count.replace(Some(note_count_fn));
        }
        Ref::map(self.cache.note_count.borrow(), |x| {
            x.as_ref().expect("`notecount_fn` can't be `None`")
        })
    }

    #[inline(always)]
    pub(crate) fn nps_max(&self) -> f64 {
        self.nps_max_fn()(&self.time)
    }

    #[inline(always)]
    pub(crate) fn note_count(&self) -> usize {
        self.note_count_fn()(&self.time)
    }

    pub(crate) fn note_count_total(&self) -> usize {
        self.note_count_fn()
            .last_key_value()
            .map(|(_, &v)| v)
            .unwrap_or(0)
    }
}

impl MidiVisualizerApp {
    /// Whether the music is currently playing.
    #[inline(always)]
    pub fn is_playing(&self) -> bool {
        self.inner.inner.is_playing()
    }

    /// Whether the video is currently being exported.
    #[inline(always)]
    pub fn is_exporting(&self) -> bool {
        self.inner.inner.is_exporting()
    }

    /// Video resolution.
    #[inline(always)]
    pub fn resolution(&self) -> Resolution {
        self.inner.inner.resolution()
    }

    #[inline(always)]
    pub fn export_config(&self) -> &Output {
        &self.inner.inner.export_config
    }

    #[inline(always)]
    pub fn visualizer_config(&self) -> &MidiVisualizerConfig {
        &self.inner.inner.visualizer_config
    }

    /// Maximum note-per-second (NPS) of the entire music before the current playing time.
    #[inline(always)]
    pub fn nps_max(&self) -> f64 {
        self.inner.nps_max()
    }

    /// Number of notes already played before the current playing time.
    #[inline(always)]
    pub fn note_count(&self) -> usize {
        self.inner.note_count()
    }

    /// Number of notes of the entire music.
    #[inline(always)]
    pub fn note_count_total(&self) -> usize {
        self.inner.note_count_total()
    }

    #[inline(always)]
    pub fn audio_device(&self) -> Option<&cpal::Device> {
        self.inner.audio_device()
    }
}

// Setters & Control

impl MidiVisualizerAppInner2 {
    pub(crate) fn play(&mut self) {
        self.synth.lock().unwrap().play();
        if self.time >= self.music.duration() {
            self.time = 0;
            self.play_start_t = Some(Instant::now());
        } else {
            self.play_start_t = Some(
                Instant::now()
                    - Duration::from_nanos((self.time as f64 / self.playback_speed) as u64),
            );
        }
    }

    pub(crate) fn pause(&mut self) {
        self.play_start_t = None;
        self.synth.lock().unwrap().pause();
        self.test_sound_playing = false;
    }

    pub(crate) fn toggle_play_pause(&mut self) {
        if self.is_playing() {
            self.pause();
        } else {
            self.play();
        }
    }

    pub(crate) fn step_frame(&mut self, n: isize) {
        if n == 0 {
            return;
        }

        let playing = self.is_playing();
        if playing {
            self.pause();
        }

        // [TODO] when the division is not exact, there can be cumulative error
        // maybe define a new `StepGrid` struct with `large_step` and `small_step` fields
        let dt = 100_000_000 / self.export_config.fps as u64 * n.unsigned_abs() as u64;
        if n >= 0 {
            self.time = (self.time + dt).min(self.duration);
        } else if self.time > dt {
            self.time -= dt;
        } else {
            self.time = 0;
        }

        if playing {
            self.play();
        }
    }

    pub(crate) fn jump_to_start(&mut self) {
        self.time = 0;
        if let Some(start_t) = &mut self.play_start_t {
            *start_t = Instant::now();
        }
    }

    pub(crate) fn jump_to_end(&mut self) {
        self.play_start_t = None;
        self.time = self.duration;
    }

    pub(crate) fn set_music(&mut self, music: MidiMusic) {
        self.pause();
        self.music = Arc::new(music);
        self.time = 0;
        self.duration = self.music.duration();
    }

    pub(crate) fn show_open_dialog(&mut self) {
        let opened_file = rfd::FileDialog::new()
            .add_filter("MIDI files", &["mid", "midi"])
            .add_filter("All files", &["*"])
            .pick_file();
        if let Some(path) = &opened_file {
            self.load_midi_file(path);
        }
    }

    pub(crate) fn load_midi_file(&mut self, path: &PathBuf) {
        match std::fs::read(path) {
            Ok(src) => match MidiMusic::try_from(src.as_slice()) {
                Ok(music) => {
                    self.set_music(music);
                    self.midi_file = Some(path.clone());
                }
                Err(err) => {
                    self.show_error_dialog(err);
                }
            },
            Err(err) => {
                self.show_error_dialog(err);
            }
        }
    }

    pub(crate) fn load_midi_bytes(&mut self, src: &[u8]) {
        match MidiMusic::try_from(src) {
            Ok(music) => {
                self.set_music(music);
            }
            Err(err) => {
                self.show_error_dialog(err);
            }
        }
    }

    pub(crate) fn show_load_style_dialog(&mut self) {
        let fd = rfd::FileDialog::new()
            .add_filter("Style config file", &["toml"])
            .add_filter("All files", &["*"]);
        if let Some(path) = fd.pick_file() {
            match std::fs::read_to_string(&path) {
                Ok(src) => match toml::de::from_str(&src) {
                    Ok(config) => self.visualizer_config = config,
                    Err(err) => self.show_error_dialog(err),
                },
                Err(err) => self.show_error_dialog(err),
            }
        }
    }

    pub(crate) fn show_save_style_dialog(&self) {
        let fd = rfd::FileDialog::new()
            .add_filter("Style config file", &["toml"])
            .add_filter("All files", &["*"]);
        if let Some(path) = fd.save_file() {
            match toml::ser::to_string_pretty(&self.visualizer_config) {
                Ok(src) => match std::fs::write(&path, src) {
                    Ok(_) => (),
                    Err(err) => self.show_error_dialog(err),
                },
                Err(err) => self.show_error_dialog(err),
            }
        }
    }

    pub(crate) fn show_revert_style_dialog(&mut self) {
        let reply = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_buttons(rfd::MessageButtons::YesNo)
            .set_title("Revert to default")
            .set_description("All current styles will be lost. Do you want to proceed?")
            .show();
        if reply == rfd::MessageDialogResult::Yes {
            self.export_config = Output::default();
            self.visualizer_config = MidiVisualizerConfig::default();
        }
    }

    pub(crate) fn show_error_dialog(&self, err: impl ToString) {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Error")
            .set_description(err.to_string())
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }

    pub(crate) fn open_audio_stream(&mut self) {
        if let Some(device) = self.audio_device() {
            match device.default_output_config() {
                Ok(config) => {
                    let synth = self.synth.clone();
                    let config = cpal::StreamConfig::from(config);
                    info!("Stream config: {:?}", config);
                    let config_clone = config.clone();
                    match device.build_output_stream(
                        &config,
                        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                            synth.lock().unwrap().write_to_buffer(&config_clone, data);
                        },
                        move |err| error!("Stream error: {}", err),
                        None,
                    ) {
                        Ok(stream) => {
                            thread::spawn(move || {
                                if let Err(err) = stream.play() {
                                    error!("Play stream error: {}", err);
                                } else {
                                    info!("Stream started.")
                                }
                                loop {
                                    thread::sleep(Duration::from_secs(1));
                                }
                            });
                        }
                        Err(err) => error!("Build stream error: {}", err),
                    }
                }
                Err(err) => error!("Stream config error: {}", err),
            }
        }
    }

    pub(crate) fn set_audio_device(&mut self, idx: isize) {
        self.audio_device_idx = idx;
        self.open_audio_stream();
    }
}

impl MidiVisualizerAppInner {
    pub(crate) fn set_music(&mut self, music: MidiMusic) {
        self.inner.set_music(music);
        self.cache.nps_max.take();
        self.cache.note_count.take();
    }
}

impl MidiVisualizerApp {
    /// Start playing the music.
    #[inline(always)]
    pub fn play(&mut self) {
        self.inner.play();
    }

    /// Pause the music.
    #[inline(always)]
    pub fn pause(&mut self) {
        self.inner.pause();
    }

    /// If the music is currently playing, then pause. Otherwise start playing.
    #[inline(always)]
    pub fn toggle_play_pause(&mut self) {
        self.inner.toggle_play_pause();
    }

    /// Step the video by `n` frames. If `n` is positive, then step forward. If `n` is negative, then step backward.
    #[inline(always)]
    pub fn step_frame(&mut self, n: isize) {
        self.inner.step_frame(n);
    }

    /// Jump to the start of the music.
    #[inline(always)]
    pub fn jump_to_start(&mut self) {
        self.inner.jump_to_start();
    }

    /// Jump to the end of the music.
    #[inline(always)]
    pub fn jump_to_end(&mut self) {
        self.inner.jump_to_end();
    }

    /// Set the currently opened music to visualize.
    #[inline(always)]
    pub fn set_music(&mut self, music: MidiMusic) {
        self.inner.set_music(music);
    }

    #[inline(always)]
    pub fn set_audio_device(&mut self, idx: isize) {
        self.inner.set_audio_device(idx);
    }

    /// Show the open file dialog to load a MIDI file.
    #[inline(always)]
    pub fn show_open_dialog(&mut self) {
        self.inner.show_open_dialog();
    }

    /// Load a MIDI file from a path. This will not open a dialog.
    #[inline(always)]
    pub fn load_midi_file(&mut self, path: &PathBuf) {
        self.inner.load_midi_file(path);
    }

    /// Load a MIDI file from raw bytes. This will not open a dialog.
    #[inline(always)]
    pub fn load_midi_bytes(&mut self, src: &[u8]) {
        self.inner.load_midi_bytes(src);
    }

    /// Show the open file dialog to load a style config TOML file.
    #[inline(always)]
    pub fn show_load_style_dialog(&mut self) {
        self.inner.show_load_style_dialog();
    }

    /// Show the save file dialog to save the current style config as TOML file.
    #[inline(always)]
    pub fn show_save_style_dialog(&self) {
        self.inner.show_save_style_dialog();
    }

    /// Show a confirm dialog to revert the current style config to default.
    #[inline(always)]
    pub fn show_revert_style_dialog(&mut self) {
        self.inner.show_revert_style_dialog();
    }

    #[inline(always)]
    pub(crate) fn show_error_dialog(&self, err: impl ToString) {
        self.inner.show_error_dialog(err);
    }
}

////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////
