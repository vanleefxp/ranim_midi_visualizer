use std::{num::NonZeroUsize, path::PathBuf};

use derivative::Derivative;
use derive_more::{Deref, DerefMut};
use gpui::*;
use indexmap::IndexSet;
use jiff::{SignedDuration, Timestamp};
use ranim::Output;
use ranim_midi_visualizer_lib::config::MidiVisualizerConfig;
use tracing::info;
use typed_floats::{self as tf, tf64};
use waveform_utils::music::{FrameRate, Metric, Music, NoteContainer as _};

#[derive(Debug, Clone)]
pub struct FileState {
    pub opened_file: Entity<Option<PathBuf>>,
    pub recent_files: Entity<IndexSet<PathBuf>>,
    pub recent_files_max_count: Option<NonZeroUsize>,
}

impl Global for FileState {}

impl FileState {
    pub fn init(cx: &mut App) {
        let opened_file = cx.new(|_| None);
        let recent_files = cx.new(|_| IndexSet::new());
        cx.set_global(Self {
            opened_file,
            recent_files,
            recent_files_max_count: Some(64.try_into().unwrap()),
        });
    }
}

pub mod file {
    use crate::menu::update_menus;

    use super::*;

    // pub fn opened_file(cx: &App) -> Entity<Option<PathBuf>> {
    //     cx.read_global::<FileState, _>(|f, _cx| f.opened_file.clone())
    // }

    pub fn set_opened_file(cx: &mut App, path: PathBuf) {
        info!("Opened file: {}", path.display());
        cx.update_global::<FileState, _>(|g, cx| {
            g.opened_file = cx.new(|_| Some(path));
        });
    }

    pub fn recent_files(cx: &App) -> Entity<IndexSet<PathBuf>> {
        cx.read_global::<FileState, _>(|f, _cx| f.recent_files.clone())
    }

    // fn update_menu(cx: &mut App) {
    //     cx.update_jump_list(menus, entries)
    // }

    pub fn add_recent_file(cx: &mut App, path: PathBuf) {
        cx.add_recent_document(&path);
        cx.update_global::<FileState, _>(|g, cx| {
            let max_count = g.recent_files_max_count;
            cx.update_entity(&g.recent_files, |v, _cx| {
                info!("\"{}\" added to recent files.", path.display());
                let (idx, inserted) = v.insert_full(path);
                let len = v.len();
                if !inserted {
                    v.swap_indices(idx, len - 1);
                }
                if let Some(max_count) = max_count {
                    let max_count = max_count.get();
                    if len > max_count {
                        v.drain(..(len - max_count))
                            .for_each(|v| info!("\"{}\" removed from recent files.", v.display()));
                    }
                }
            });
        });
        update_menus(cx);
    }

    pub fn clear_recent_files(cx: &mut App) {
        cx.update_global::<FileState, _>(|g, cx| {
            cx.update_entity(&g.recent_files, |v, _cx| {
                v.drain(..)
                    .for_each(|v| info!("\"{}\" removed from recent files.", v.display()));
            });
        });
        update_menus(cx);
    }
}

#[derive(Debug, Clone)]
pub struct MusicDataState {
    pub music: Entity<Music>,
}

impl Global for MusicDataState {}

impl MusicDataState {
    pub fn init(cx: &mut App) {
        let music = cx.new(|_| Music::default());
        cx.set_global(Self { music });
    }
}

pub mod music_data {
    use super::*;

    pub fn music(cx: &App) -> Entity<Music> {
        cx.read_global::<MusicDataState, _>(|g, _cx| g.music.clone())
    }

    pub fn set_music(cx: &mut App, music: Music) {
        info!(
            "Music loaded: {} notes, {} time units, time resolution {}.",
            music.as_unmapped().note_count(),
            music.duration(),
            music.time_resolution,
        );
        cx.update_global::<MusicDataState, _>(|g, cx| {
            g.music = cx.new(|_| music);
        });
        playback::refresh(cx);
        playback::pause(cx);
        playback::jump_to_time(cx, 0);
    }
}

#[derive(Debug, Clone)]
pub struct VideoConfigState {
    pub visualizer_config: Entity<MidiVisualizerConfig>,
    pub export_config: Entity<Output>,
    pub clear_color: Rgba,
}

impl Global for VideoConfigState {}

impl VideoConfigState {
    pub fn init(cx: &mut App) {
        let visualizer_config = cx.new(|_| MidiVisualizerConfig::default());
        let export_config = cx.new(|_| Output::default());
        cx.set_global(Self {
            visualizer_config,
            export_config,
            clear_color: rgb(0x282c34),
        });
    }
}

pub mod video_config {
    use super::*;

    pub fn visualizer_config(cx: &App) -> Entity<MidiVisualizerConfig> {
        cx.read_global::<VideoConfigState, _>(|v, _cx| v.visualizer_config.clone())
    }

    pub fn export_config(cx: &App) -> Entity<Output> {
        cx.read_global::<VideoConfigState, _>(|v, _cx| v.export_config.clone())
    }

    pub fn clear_color(cx: &App) -> Rgba {
        cx.read_global::<VideoConfigState, _>(|v, _cx| v.clear_color)
    }

    pub fn revert_to_default(cx: &mut App) {
        info!("Reverted to default style.");
        VideoConfigState::init(cx);
        playback::refresh(cx);
    }
}

#[derive(Derivative)]
#[derivative(Debug, Clone, Default)]
pub struct PlaybackState {
    #[derivative(Default(value = "1_000_000_000.try_into().unwrap()"))]
    pub time_resolution: FrameRate,
    pub time: Metric,
    pub max_time: Metric,
    #[derivative(Default(value = "tf::as_const!(StrictlyPositiveFinite, 1f64)"))]
    pub playback_speed: tf64::StrictlyPositiveFinite,
    pub play_start_time: Option<Timestamp>,
    pub looping: bool,
    #[derivative(Default(value = "60"))]
    pub stepping_framerate: u32,
}

impl Global for PlaybackState {}

impl PlaybackState {
    pub fn init(cx: &mut App) {
        cx.set_global(Self::default());
    }

    pub fn is_playing(&self, _cx: &App) -> bool {
        self.play_start_time.is_some()
    }

    fn time_unit_to_duration(&self, _cx: &App, time_unit: Metric) -> SignedDuration {
        SignedDuration::from_secs_f64(time_unit as f64 / self.time_resolution.get() as f64)
    }

    fn duration_to_time_unit(&self, _cx: &App, duration: SignedDuration) -> Metric {
        (duration.as_secs_f64() * self.time_resolution.get() as f64) as Metric
    }

    fn get_play_start_time(&self, cx: &App, time_unit: Metric) -> Timestamp {
        Timestamp::now()
            - self
                .time_unit_to_duration(cx, time_unit)
                .div_f64(self.playback_speed.into())
    }

    pub fn play(&mut self, cx: &App) {
        info!("Start playing...");
        self.play_start_time = Some(self.get_play_start_time(cx, self.time));
    }

    pub fn pause(&mut self, _cx: &App) {
        info!("Paused.");
        self.play_start_time = None;
    }

    pub fn jump_to_time(&mut self, cx: &mut App, time: Metric) {
        info!("Jump to time {}.", time);
        self.time = time;
        if self.is_playing(cx) {
            self.play_start_time = Some(self.get_play_start_time(cx, time));
        }
    }

    pub fn update_playback(&mut self, cx: &mut App) {
        if let Some(play_start_time) = self.play_start_time {
            let now = Timestamp::now();
            let cur_time = now
                .duration_since(play_start_time)
                .mul_f64(self.playback_speed.into());
            let cur_time = self.duration_to_time_unit(cx, cur_time);
            if cur_time > self.max_time {
                if self.looping {
                    self.time = cur_time.rem_euclid(self.max_time)
                } else {
                    self.time = self.max_time;
                    self.pause(cx);
                }
            } else {
                self.time = cur_time;
            }
        }
    }

    pub fn step_frame(&mut self, cx: &mut App, n_frames: isize) {
        let time_diff = (self.time_resolution.get() as f64 / self.stepping_framerate as f64
            * n_frames as f64) as Metric;
        self.jump_to_time(cx, (self.time + time_diff).clamp(0, self.max_time));
    }
}

pub mod playback {
    use super::*;

    pub fn refresh(cx: &mut App) {
        cx.update_global::<PlaybackState, _>(|g, cx| {
            g.time_resolution = cx.read_entity(&music_data::music(cx), |v, _cx| v.time_resolution);
            g.max_time = cx.read_entity(&music_data::music(cx), |v, _cx| v.duration());
            g.stepping_framerate = cx.read_entity(&video_config::export_config(cx), |v, _cx| v.fps);
            info!("Playback data kept up with music and video config.");
            info!(
                "Time resolution: {} units per second, max time: {} units, stepping framerate: {} fps",
                g.time_resolution,
                g.max_time,
                g.stepping_framerate
            );
        });
    }

    // pub fn time_resolution(cx: &App) -> FrameRate {
    //     cx.read_global::<PlaybackState, _>(|v, _cx| v.time_resolution)
    // }

    pub fn is_playing(cx: &App) -> bool {
        cx.read_global::<PlaybackState, _>(|g, cx| g.is_playing(cx))
    }

    pub fn time(cx: &App) -> Metric {
        cx.read_global::<PlaybackState, _>(|g, _cx| g.time)
    }

    pub fn max_time(cx: &App) -> Metric {
        cx.read_global::<PlaybackState, _>(|g, _cx| g.max_time)
    }

    pub fn is_looping(cx: &App) -> bool {
        cx.read_global::<PlaybackState, _>(|g, _cx| g.looping)
    }

    pub fn play(cx: &mut App) {
        cx.update_global::<PlaybackState, _>(|g, cx| g.play(cx));
    }

    pub fn pause(cx: &mut App) {
        cx.update_global::<PlaybackState, _>(|g, cx| g.pause(cx));
    }

    pub fn jump_to_time(cx: &mut App, time: Metric) {
        cx.update_global::<PlaybackState, _>(|g, cx| g.jump_to_time(cx, time));
    }

    pub fn update_playback(cx: &mut App) {
        cx.update_global::<PlaybackState, _>(|g, cx| g.update_playback(cx));
    }

    pub fn step_frame(cx: &mut App, n_frames: isize) {
        cx.update_global::<PlaybackState, _>(|g, cx| g.step_frame(cx, n_frames));
    }
}

#[derive(Debug, Clone, Copy, Deref, DerefMut, Default)]
pub struct ShouldReloadMenuBar(pub bool);

impl Global for ShouldReloadMenuBar {}
