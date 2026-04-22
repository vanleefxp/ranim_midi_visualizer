mod utils;
pub mod widgets;

use crate::{
    utils::{MidiVisualizerConfig, egui_color_to_hex_string, nano_to_time_string},
    widgets::MidiVisualizerPreview,
};
use async_channel::Receiver;
use eframe::egui::{self, Widget as _};
use egui_dock::TabViewer as _;
use enum_ordinalize::Ordinalize;
use ranim::{
    Output, OutputFormat, RanimScene, SceneConfig,
    cmd::{preview::Resolution, render_scene_output_with_progress},
};
use ranim_midi_visualizer_lib::{ColorBy, midi_visualizer_scene};
use ranim_midi_visualizer_math::func::LadderFn;
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use std::{
    cell::{Ref, RefCell},
    collections::HashMap,
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use structured_midi::MidiMusic;
use tracing::{error, info};

#[allow(unused)]
enum ExportProgress {
    /// (current_frame, total_frames)
    Progress(u64, u64),
    Done,
    Error(String),
}

#[derive(Clone, Debug)]
pub struct MidiVisualizerAppInner2 {
    midi_file: Option<PathBuf>,
    soundfont_file: Option<PathBuf>,
    synth_settings: SynthesizerSettings,
    synth: Option<Arc<Mutex<Synthesizer>>>,

    /// the displaying MIDI music
    pub music: Arc<MidiMusic>,
    /// soundfont for playing MIDI notes
    pub soundfont: Option<Arc<SoundFont>>,

    /// configuration of the MIDI visualizer
    pub visualizer_config: MidiVisualizerConfig,
    /// scene clear color
    pub clear_color: egui::Color32,
    /// current playing time in nanoseconds
    pub time: u64,
    pub looping: bool,
    /// absolute time corresponding to the start of music
    ///
    /// When "play" button is clicked, this value is set to the instant of now minus the song's current playing time.
    pub play_start_t: Option<Instant>,

    /// time window for calculating NPS and legato index
    time_window: u64,
    /// total duration of the music
    duration: u64,
    /// video playback speed
    playback_speed: f64,

    // Export
    export_config: Output,
    export_progress_rx: Option<Receiver<ExportProgress>>,
    export_progress: (u64, u64),
}

impl Default for MidiVisualizerAppInner2 {
    fn default() -> Self {
        Self {
            midi_file: None,
            soundfont_file: None,
            synth_settings: SynthesizerSettings::new(44100),
            synth: None,

            music: Default::default(),
            soundfont: None,

            visualizer_config: Default::default(),
            clear_color: egui::Color32::from_rgb(0x28, 0x2c, 0x34), // #282c34
            time: 0,
            looping: false,
            play_start_t: None,

            time_window: 1_000_000_000, // 1 second
            duration: 0,
            playback_speed: 1.0,

            export_config: Default::default(),
            export_progress_rx: None,
            export_progress: (0, 0),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MidiVisualizerAppInner {
    inner: MidiVisualizerAppInner2,

    /// cache for NPS max function
    nps_max_cache: RefCell<Option<LadderFn<u64, f64>>>,
    note_count_cache: RefCell<Option<LadderFn<u64, usize>>>,
    added_tab: RefCell<Option<(MidiVisualizerTab, egui_dock::NodePath)>>,
    // synth: RefCell<Option<Synthesizer>>,
    visible_tabs: RefCell<HashMap<MidiVisualizerTab, egui_dock::NodePath>>,
}

impl Deref for MidiVisualizerAppInner {
    type Target = MidiVisualizerAppInner2;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for MidiVisualizerAppInner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub struct MidiVisualizerApp {
    inner: MidiVisualizerAppInner,
    dock_state: egui_dock::DockState<MidiVisualizerTab>,
}

impl Default for MidiVisualizerApp {
    fn default() -> Self {
        let value = Self {
            inner: Default::default(),
            dock_state: Self::default_dock_state(),
        };
        value.update_visible_tabs();

        value
    }
}

impl Deref for MidiVisualizerApp {
    type Target = MidiVisualizerAppInner;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for MidiVisualizerApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub enum AppStatus {
    #[default]
    NoFileOpened,
    FileOpened(PathBuf),
    ReadingFailed(PathBuf),
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Ordinalize)]
#[non_exhaustive]
pub enum MidiVisualizerTab {
    VideoPlayback,
    StyleSettings,
    OutputSettings,
    AudioSettings,
}

const TAB_TITLES: [&str; MidiVisualizerTab::VARIANT_COUNT] = [
    "Video Playback",
    "Style Settings",
    "Output Settings",
    "Audio Settings",
];
const TAB_ICONS: [&str; MidiVisualizerTab::VARIANT_COUNT] = [
    egui_phosphor::regular::VIDEO,
    egui_phosphor::regular::PAINT_BRUSH,
    egui_phosphor::regular::FILE_VIDEO,
    egui_phosphor::regular::MICROPHONE,
];

impl MidiVisualizerTab {
    #[inline(always)]
    pub fn title(&self) -> &'static str {
        TAB_TITLES[*self as usize]
    }
    #[inline(always)]
    pub fn icon(&self) -> &'static str {
        TAB_ICONS[*self as usize]
    }
}

impl MidiVisualizerAppInner2 {
    pub fn play(&mut self) {
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

    pub fn pause(&mut self) {
        self.play_start_t = None;
    }

    pub fn is_playing(&self) -> bool {
        self.play_start_t.is_some()
    }

    pub fn is_exporting(&self) -> bool {
        self.export_progress_rx.is_some()
    }

    pub fn toggle_play_pause(&mut self) {
        if self.is_playing() {
            self.pause();
        } else {
            self.play();
        }
    }

    pub fn step_frame(&mut self, n: isize) {
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

    pub fn jump_to_start(&mut self) {
        self.time = 0;
        if let Some(start_t) = &mut self.play_start_t {
            *start_t = Instant::now();
        }
    }

    pub fn jump_to_end(&mut self) {
        self.play_start_t = None;
        self.time = self.duration;
    }

    pub fn set_music(&mut self, music: MidiMusic) {
        self.pause();
        self.music = Arc::new(music);
        self.time = 0;
        self.duration = self.music.duration();
    }

    fn start_export(&mut self, ctx: egui::Context) {
        let (progress_tx, progress_rx) = async_channel::unbounded();
        self.export_progress_rx = Some(progress_rx);

        let music = self.music.clone();
        let visualizer_config = self.visualizer_config.clone().into();
        let scene_config = SceneConfig {
            clear_color: egui_color_to_hex_string(self.clear_color),
        };
        let output = self.export_config.clone();
        let resolution = self.resolution();

        let constructor = move |r: &mut RanimScene| {
            midi_visualizer_scene(r, music.as_ref(), &visualizer_config, resolution);
        };

        std::thread::spawn(move || {
            let progress_tx_cb = progress_tx.clone();
            let ctx_cb = ctx.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                render_scene_output_with_progress(
                    constructor,
                    "midi-visualizer-scene".to_string(),
                    &scene_config,
                    &output,
                    2,
                    Some(Box::new(move |current, total| {
                        let _ =
                            progress_tx_cb.send_blocking(ExportProgress::Progress(current, total));
                        ctx_cb.request_repaint();
                    })),
                );

                let _ = progress_tx.send_blocking(ExportProgress::Done);
                ctx.request_repaint();
            }));

            if let Err(e) = result {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown export error".to_string()
                };
                let _ = progress_tx.send_blocking(ExportProgress::Error(msg));
                ctx.request_repaint();
            }
        });
    }

    fn resolution(&self) -> Resolution {
        Resolution {
            width: self.export_config.width,
            height: self.export_config.height,
        }
    }

    fn show_open_dialog(&mut self) {
        let opened_file = rfd::FileDialog::new()
            .add_filter("MIDI files", &["mid", "midi"])
            .add_filter("All files", &["*"])
            .pick_file();
        if let Some(path) = &opened_file {
            self.load_midi_file(path);
        }
    }

    fn load_midi_file(&mut self, path: &PathBuf) {
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

    fn load_midi_bytes(&mut self, src: &[u8]) {
        match MidiMusic::try_from(src) {
            Ok(music) => {
                self.set_music(music);
            }
            Err(err) => {
                self.show_error_dialog(err);
            }
        }
    }

    fn load_soundfont_file(&mut self, path: &PathBuf) {
        match std::fs::File::open(path) {
            Ok(ref mut file) => match SoundFont::new(file) {
                Ok(soundfont) => {
                    self.soundfont = Some(Arc::new(soundfont));
                    self.soundfont_file = Some(path.clone());
                }
                Err(err) => self.show_error_dialog(err),
            },
            Err(err) => self.show_error_dialog(err),
        }
    }

    fn show_export_dialog(&mut self, ctx: egui::Context) {
        let mut fd = rfd::FileDialog::new()
            .add_filter("MP4", &["mp4"])
            .add_filter("WEBM", &["webm"])
            .add_filter("MOV", &["mov"])
            .add_filter("GIF", &["gif"])
            .set_title("Save video");

        if let Some(path) = &self.midi_file {
            if let Some(parent) = path.parent() {
                fd = fd.set_directory(parent);
            }
            if let Some(filename) = path.file_stem()
                && let Some(filename) = filename.to_str()
            {
                fd = fd.set_file_name(filename);
            }
        }

        let path = fd.save_file();
        if let Some(path) = path
            && let Some(ext) = path.extension()
            && let Some(ext) = ext.to_str()
        {
            use OutputFormat::*;
            let format = {
                let ext = ext.to_lowercase();
                match ext.as_str() {
                    "mp4" => Mp4,
                    "webm" => Webm,
                    "mov" => Mov,
                    "gif" => Gif,
                    _ => unreachable!(),
                }
            };
            self.export_config.format = format;

            // Warn if the current video format does not support opacity but the clear color is not opaque
            if !self.clear_color.is_opaque() && matches!(format, Mp4 | Gif) {
                let result = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("Opacity Warning")
                .set_description(format!("The {} format does not support opacity. The background color will be blended with black. Do you really want to proceed?", format))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
                if result == rfd::MessageDialogResult::No {
                    // re-ask for export path
                    self.show_export_dialog(ctx);
                    return;
                }
            }

            if let Some(filename) = path.file_stem()
                && let Some(filename) = filename.to_str()
            {
                self.export_config.name = Some(filename.to_string());
            }
            self.export_config.dir = path
                .parent()
                .map(|v| v.display().to_string())
                .unwrap_or_else(|| ".".to_string());
            self.start_export(ctx);
        }
    }

    fn show_load_style_dialog(&mut self) {
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

    fn show_save_style_dialog(&self) {
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

    fn show_revert_style_dialog(&mut self) {
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

    fn show_load_soundfont_dialog(&mut self) {
        let fd = rfd::FileDialog::new()
            .add_filter("Soundfont file", &["sf2", "sf3"])
            .add_filter("All", &["*"]);
        if let Some(path) = fd.pick_file() {
            self.load_soundfont_file(&path);
        }
    }

    fn show_error_dialog(&self, err: impl ToString) {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Error")
            .set_description(err.to_string())
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

impl MidiVisualizerAppInner {
    pub fn set_music(&mut self, music: MidiMusic) {
        self.inner.set_music(music);
        self.clear_cache();
    }

    fn clear_cache(&self) {
        self.nps_max_cache.take();
        self.note_count_cache.take();
    }

    fn nps_max_fn(&self) -> Ref<'_, LadderFn<u64, f64>> {
        if self.nps_max_cache.borrow().is_none() {
            let nps_max_fn = self.music.nps_max_fn(self.time_window);
            self.nps_max_cache.replace(Some(nps_max_fn));
        }
        Ref::map(self.nps_max_cache.borrow(), |x| {
            x.as_ref().expect("`nps_max_fn` can't be `None`")
        })
    }

    fn note_count_fn(&self) -> Ref<'_, LadderFn<u64, usize>> {
        if self.note_count_cache.borrow().is_none() {
            let note_count_fn = self.music.note_count_fn();
            self.note_count_cache.replace(Some(note_count_fn));
        }
        Ref::map(self.note_count_cache.borrow(), |x| {
            x.as_ref().expect("`notecount_fn` can't be `None`")
        })
    }

    fn note_count_total(&self) -> usize {
        self.note_count_fn()
            .last_key_value()
            .map(|(_, &v)| v)
            .unwrap_or(0)
    }
}

impl egui_dock::TabViewer for MidiVisualizerAppInner {
    type Tab = MidiVisualizerTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        format!("{} {}", tab.icon(), tab.title()).into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        use MidiVisualizerTab::*;
        match *tab {
            VideoPlayback => self.video_playback_ui(ui),
            StyleSettings => self.style_settings_ui(ui),
            OutputSettings => self.output_settings_ui(ui),
            AudioSettings => self.audio_settings_ui(ui),
        }
    }

    fn scroll_bars(&self, tab: &Self::Tab) -> [bool; 2] {
        use MidiVisualizerTab::*;
        match tab {
            VideoPlayback => [false, false],
            _ => [true, true],
        }
    }

    #[allow(clippy::match_like_matches_macro)]
    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        use MidiVisualizerTab::*;
        match tab {
            VideoPlayback => false,
            _ => true,
        }
    }

    fn on_add(&mut self, _path: egui_dock::NodePath) {}

    fn add_popup(&mut self, ui: &mut egui::Ui, node_path: egui_dock::NodePath) {
        let mut visible_tabs = self.visible_tabs.borrow_mut();
        for tab in MidiVisualizerTab::VARIANTS.iter().copied() {
            let path = visible_tabs.get(&tab).copied();
            if path.is_none() {
                let resp = ui.selectable_label(false, format!("{} {}", tab.icon(), tab.title()));
                if resp.clicked() {
                    visible_tabs.insert(tab, node_path);
                    self.added_tab.replace(Some((tab, node_path)));
                }
            }
        }
    }
}

impl eframe::App for MidiVisualizerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // egui::Window::new("Debug")
        //     .scroll([false, true])
        //     .max_size(egui::vec2(800., 600.))
        //     .id(egui::Id::new("debug_window"))
        //     .show(ui, |ui| {
        //         let mut text = format!("{:?}", self.nps_max_cache);
        //         egui::TextEdit::multiline(&mut text).ui(ui);
        //         let mut text = format!("{:?}", self.note_count_cache);
        //         egui::TextEdit::multiline(&mut text).ui(ui);
        //     });

        // exporting
        {
            // Poll export progress
            if let Some(rx) = &self.inner.inner.export_progress_rx {
                let mut done = false;
                let mut error_msg = None;

                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        ExportProgress::Progress(current, total) => {
                            self.inner.inner.export_progress = (current, total);
                        }
                        ExportProgress::Done => {
                            done = true;
                        }
                        ExportProgress::Error(err) => {
                            error_msg = Some(err);
                            done = true;
                        }
                    }
                }

                if done {
                    self.export_progress_rx = None;
                    self.export_progress = (0, 0);
                    if let Some(err) = error_msg {
                        error!("Export failed: {err}");
                    } else {
                        info!("Export completed");
                    }
                } else {
                    ctx.request_repaint();
                }
            }

            // export dialog
            if self.is_exporting() {
                egui::Window::new("Export")
                    .id(egui::Id::new("export_window"))
                    .collapsible(false)
                    .max_width(300.)
                    .title_bar(false)
                    .resizable(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Exporting to:");
                            ui.code(format!(
                                "{}/{}_{}x{}_{}.{}",
                                self.export_config.dir,
                                self.export_config.name.as_deref().unwrap_or(""),
                                self.export_config.width,
                                self.export_config.height,
                                self.export_config.fps,
                                self.export_config.format,
                            ));
                        });
                        let (current, total) = self.export_progress;
                        if total > 0 {
                            let progress = current as f32 / total as f32;
                            egui::ProgressBar::new(progress)
                                .text(format!(
                                    "{current}/{total} frames ({:.0}%)",
                                    progress * 100.0
                                ))
                                .ui(ui);
                        } else {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Preparing...");
                            });
                        }
                    });
            }
        }

        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            egui::MenuBar::default().ui(ui, |ui| self.menu_ui(ui));
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).inner_margin(0))
            .show_inside(ui, |ui| {
                let num_visible_tabs = self.visible_tabs.borrow().len();
                let show_add_popup = num_visible_tabs < MidiVisualizerTab::VARIANT_COUNT;

                egui_dock::DockArea::new(&mut self.dock_state)
                    .show_leaf_collapse_buttons(false)
                    .show_add_buttons(show_add_popup)
                    .show_add_popup(show_add_popup)
                    .show_inside(ui, &mut self.inner);

                if let Some((tab, path)) = self.added_tab.take() {
                    self.dock_state.set_focused_node_and_surface(path);
                    self.dock_state.push_to_focused_leaf(tab);
                }

                self.update_visible_tabs();
                // [TODO] only update visible tabs when needed
            });
    }
}

impl MidiVisualizerAppInner {
    fn video_playback_ui(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().visuals.code_bg_color = egui::Color32::TRANSPARENT;
        let ctx = ui.ctx().clone();

        // Space bar toggles play / pause
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.toggle_play_pause();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            self.step_frame(-1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            self.step_frame(1);
        }

        // drag and drop for midi files
        {
            let file = ctx.input(|i| i.raw.dropped_files.first().cloned());
            if let Some(file) = file {
                if let Some(src) = file.bytes {
                    self.load_midi_bytes(src.as_ref());
                } else if let Some(path) = file.path {
                    self.load_midi_file(&path);
                }
            }
        }

        egui::Panel::bottom("playback_control").show_inside(ui, |ui| {
            // Playback control
            egui::MenuBar::default().ui(ui, |ui| {
                // Jump to start
                {
                    let resp = ui
                        .button(egui_phosphor::regular::SKIP_BACK)
                        .on_hover_text("Jump to start");
                    if resp.clicked() {
                        self.jump_to_start();
                    }
                }

                // Step back 1 frame
                {
                    let resp = ui
                        .button(egui_phosphor::regular::CARET_LEFT)
                        .on_hover_text("Step back 1 frame");
                    if resp.clicked() {
                        self.step_frame(-1);
                    }
                }

                // Play / pause button
                {
                    if let Some(start_t) = self.play_start_t {
                        if ui
                            .selectable_label(true, egui_phosphor::regular::PAUSE)
                            .on_hover_text("Pause")
                            .clicked()
                        {
                            self.pause();
                        } else {
                            // currently playing
                            let new_time = ((Instant::now() - start_t).as_nanos() as f64
                                * self.playback_speed)
                                as u64;
                            if new_time > self.duration {
                                if self.looping {
                                    // restarts from beginning
                                    self.time = new_time % self.duration;
                                } else {
                                    // pauses at final state
                                    self.time = self.duration;
                                    self.play_start_t = None;
                                }
                            } else {
                                self.time = new_time;
                            }
                            ui.request_repaint();
                        }
                    } else {
                        if ui
                            .selectable_label(false, egui_phosphor::regular::PLAY)
                            .on_hover_text("Play")
                            .clicked()
                        {
                            self.play();
                        }
                    }
                }

                // Step forward 1 frame
                {
                    let resp = ui
                        .button(egui_phosphor::regular::CARET_RIGHT)
                        .on_hover_text("Step back 1 frame");
                    if resp.clicked() {
                        self.step_frame(1);
                    }
                }

                // Jump to end
                if ui
                    .button(egui_phosphor::regular::SKIP_FORWARD)
                    .on_hover_text("Jump to end")
                    .clicked()
                {
                    self.jump_to_end();
                }

                ui.separator();

                // Looping toggle
                if ui
                    .selectable_label(self.looping, egui_phosphor::regular::REPEAT)
                    .on_hover_text("Looping on / off")
                    .clicked()
                {
                    self.looping = !self.looping;
                }

                // Playback speed edit
                {
                    let mut resp = egui::DragValue::new(&mut self.playback_speed)
                        .range(0.1f64..=20.)
                        .speed(0.01)
                        .custom_formatter(|value, _| format!("{value:.2}×"))
                        .update_while_editing(false)
                        .ui(ui)
                        .on_hover_text("Playback speed");

                    // change back to 1x on double click
                    if resp.double_clicked() {
                        self.playback_speed = 1.;
                        resp.mark_changed();
                    }

                    if resp.changed()
                        && let Some(start_t) = &mut self.inner.play_start_t
                    {
                        *start_t = Instant::now()
                            - Duration::from_nanos(
                                (self.inner.time as f64 / self.inner.playback_speed) as u64,
                            );
                    }
                }

                ui.separator();

                // time display
                ui.code(nano_to_time_string(self.time));

                // time slider
                {
                    ui.style_mut().spacing.slider_width = ui.available_width();
                    let slider = egui::Slider::new(&mut self.inner.time, 0..=self.inner.duration)
                        .show_value(false)
                        .handle_shape(egui::style::HandleShape::Circle);
                    let resp = ui.add_enabled(self.inner.duration > 0, slider);
                    if resp.changed() && self.is_playing() {
                        self.play();
                    }
                }
            });
        });

        // Preview area
        egui::CentralPanel::default().show_inside(ui, |ui| {
            let mut preview_widget = MidiVisualizerPreview::new(
                &self.music,
                &self.visualizer_config,
                self.clear_color,
                self.resolution(),
            );
            preview_widget.time = self.time;

            let cache = &mut preview_widget.cache;
            cache.note_count = Some(self.note_count_fn()(&self.time));
            cache.note_count_total = Some(self.note_count_total());
            cache.nps_max = Some(self.nps_max_fn()(&self.time));

            preview_widget.ui(ui);
        });
    }

    fn style_settings_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("{} Playback", egui_phosphor::regular::VIDEO_CAMERA))
                .heading(),
        )
        .id_salt("playback_collapsible")
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("playback_grid").show(ui, |ui| {
                // Scroll speed
                {
                    let value = &mut self.inner.visualizer_config.scroll_speed;
                    ui.label("Note scroll speed: ");
                    ui.horizontal(|ui| {
                        egui::DragValue::new(value)
                            .range(0.1..=5.0)
                            .speed(0.01)
                            .max_decimals(3)
                            .ui(ui);
                        ui.label("Ranim units / s");
                    });
                    ui.end_row();
                    ui.label("");
                    ui.label("video height = 8 Ranim units");
                    ui.end_row();
                }

                // Time window
                {
                    let value = &mut self.inner.visualizer_config.time_window;
                    ui.label("Time window:");
                    ui.horizontal(|ui| {
                        let resp = egui::DragValue::new(value)
                            .range(100_000_000u64..=5_000_000_000)
                            .speed(1e7)
                            .custom_parser(|s| {
                                let seconds: f64 = s.parse().ok()?;
                                if seconds > 0. {
                                    Some(seconds * 1e9)
                                } else {
                                    None
                                }
                            })
                            .custom_formatter(|nanos, _| format!("{:.2}", nanos / 1e9))
                            .update_while_editing(false)
                            .ui(ui);
                        if resp.drag_stopped() {
                            self.nps_max_cache.borrow_mut().take();
                        }
                        ui.label("s");
                    });
                    ui.end_row();
                }
            });
        });

        egui::CollapsingHeader::new(
            egui::RichText::new(format!("{} Colors", egui_phosphor::regular::PALETTE)).heading(),
        )
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("color_grid").show(ui, |ui| {
                // Clear color
                {
                    ui.label("Clear color:");
                    egui::color_picker::color_edit_button_srgba(
                        ui,
                        &mut self.inner.clear_color,
                        egui::color_picker::Alpha::BlendOrAdditive,
                    );
                    ui.end_row();
                }

                // Note colors
                {
                    ui.label("Note colors:");
                    ui.horizontal(|ui| {
                        let note_colors = &mut self.inner.visualizer_config.colors;
                        let color_by = self.inner.visualizer_config.color_by;
                        for (i, color) in note_colors.iter_mut().enumerate() {
                            let mut resp = egui::color_picker::color_edit_button_srgba(
                                ui,
                                color,
                                egui::color_picker::Alpha::Opaque,
                            );
                            use ColorBy::*;
                            match color_by {
                                Channel => resp = resp.on_hover_text(format!("Channel {}", i + 1)),
                                Track => resp = resp.on_hover_text(format!("Track {}", i + 1)),
                                KeyColor => match i {
                                    0 => resp = resp.on_hover_text("White key color"),
                                    1 => resp = resp.on_hover_text("Black key color"),
                                    _ => (),
                                },
                            }
                        }
                        ui.spacing();
                        // [TODO] drag to swap colors, delete one specific color

                        // plus button: add color
                        {
                            let resp = ui
                                .button(egui_phosphor::regular::PLUS)
                                .on_hover_text("New color");
                            if resp.clicked() {
                                self.visualizer_config.colors.push(egui::Color32::WHITE);
                            }
                        }

                        // minus button: delete color
                        {
                            let resp = ui
                                .add_enabled(
                                    self.visualizer_config.colors.len() > 1,
                                    egui::Button::new(egui_phosphor::regular::MINUS),
                                )
                                .on_hover_text("Delete last color");
                            if resp.clicked() {
                                self.visualizer_config.colors.pop();
                            }
                        }
                    });
                    ui.end_row();
                }

                // Color by
                {
                    let color_by = self.inner.visualizer_config.color_by;
                    let color_by_text = |color_by: ColorBy| match color_by {
                        ColorBy::Channel => "Channel",
                        ColorBy::Track => "Track",
                        ColorBy::KeyColor => "White / black key",
                    };

                    ui.label("Note colors by:");
                    egui::ComboBox::from_id_salt("color_by_combo")
                        .selected_text(color_by_text(color_by))
                        .show_ui(ui, |ui| {
                            use ColorBy::*;
                            let color_by = &mut self.inner.visualizer_config.color_by;
                            for value in [Channel, Track, KeyColor] {
                                ui.selectable_value(color_by, value, color_by_text(value));
                            }
                        });
                    ui.end_row();
                }

                // Key colors
                {
                    ui.label("Key colors:");
                    ui.horizontal(|ui| {
                        let keyboard_color = &mut self.visualizer_config.keyboard_config.color;
                        for (color, text) in keyboard_color
                            .key_color
                            .iter_mut()
                            .zip(["White key color", "Black key color"])
                        {
                            egui::color_picker::color_edit_button_srgba(
                                ui,
                                color,
                                egui::color_picker::Alpha::BlendOrAdditive,
                            )
                            .on_hover_text(text);
                        }
                        ui.spacing();
                        egui::color_picker::color_edit_button_srgba(
                            ui,
                            &mut keyboard_color.stroke_color,
                            egui::color_picker::Alpha::BlendOrAdditive,
                        )
                        .on_hover_text("Stroke color");
                    });
                    ui.end_row();
                }

                // Status bar color
                {
                    let status_bar_config = &mut self.visualizer_config.status_bar_config;
                    ui.label("Status bar colors: ");
                    ui.horizontal(|ui| {
                        egui::color_picker::color_edit_button_srgba(
                            ui,
                            &mut status_bar_config.fg_color,
                            egui::color_picker::Alpha::BlendOrAdditive,
                        )
                        .on_hover_text("Foreground color");
                        egui::color_picker::color_edit_button_srgba(
                            ui,
                            &mut status_bar_config.bg_color,
                            egui::color_picker::Alpha::BlendOrAdditive,
                        )
                        .on_hover_text("Background color");
                    });
                    ui.end_row();
                }

                // Progress bar color
                {
                    let progress_bar_config = &mut self.visualizer_config.progress_bar_config;
                    ui.label("Progress bar colors: ");
                    ui.horizontal(|ui| {
                        egui::color_picker::color_edit_button_srgba(
                            ui,
                            &mut progress_bar_config.fg_color,
                            egui::color_picker::Alpha::BlendOrAdditive,
                        )
                        .on_hover_text("Text color");
                        egui::color_picker::color_edit_button_srgba(
                            ui,
                            &mut progress_bar_config.bg_color,
                            egui::color_picker::Alpha::BlendOrAdditive,
                        )
                        .on_hover_text("Background color");
                    });
                    ui.end_row();
                }
            });
        });

        egui::CollapsingHeader::new(
            egui::RichText::new(format!("{} Size", egui_phosphor::regular::RULER)).heading(),
        )
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("size_grid").show(ui, |ui| {
                {
                    let piano_keyboard_size =
                        &mut self.inner.visualizer_config.keyboard_config.size;

                    // Key size
                    {
                        ui.label("Key size: ");
                        ui.horizontal(|ui| {
                            egui::DragValue::new(&mut piano_keyboard_size.white_height)
                                .range(0.0..=5.0)
                                .speed(0.01)
                                .max_decimals(3)
                                .ui(ui)
                                .on_hover_text("White key height");
                            egui::DragValue::new(&mut piano_keyboard_size.black_size.x)
                                .range(0.5..=1.0)
                                .speed(0.01)
                                .max_decimals(3)
                                .ui(ui)
                                .on_hover_text("Black key width");
                            egui::DragValue::new(&mut piano_keyboard_size.black_size.y)
                                .range(0.0..=piano_keyboard_size.white_height)
                                .speed(0.01)
                                .max_decimals(2)
                                .ui(ui)
                                .on_hover_text("Black key height");
                        });
                        ui.end_row();
                        ui.label("");
                        ui.label("Unit: white key width");
                        ui.end_row();
                    }

                    // Black key offset
                    {
                        let black_offset = &mut piano_keyboard_size.black_offset;
                        ui.label("Black key offset: ");
                        ui.horizontal(|ui| {
                            for value in black_offset.iter_mut() {
                                egui::DragValue::new(value)
                                    .range(-1.0..=1.0)
                                    .speed(0.01)
                                    .max_decimals(3)
                                    .ui(ui);
                            }
                            {
                                let resp = ui
                                    .button(egui_phosphor::regular::FLIP_HORIZONTAL)
                                    .on_hover_text("Make symmetric");
                                if resp.clicked() {
                                    black_offset[1] = -black_offset[0];
                                    black_offset[4] = -black_offset[2];
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("");
                        ui.label("Unit: black key half width");
                        ui.end_row();
                    }

                    // Note horizontal scale
                    {
                        let note_h_scale = &mut piano_keyboard_size.note_h_scale;
                        ui.label("Note horizontal scale: ");
                        ui.horizontal(|ui| {
                            for (value, text) in
                                note_h_scale.iter_mut().zip(["White key", "Black key"])
                            {
                                egui::DragValue::new(value)
                                    .range(0.0..=1.0)
                                    .speed(0.01)
                                    .max_decimals(3)
                                    .ui(ui)
                                    .on_hover_text(text);
                            }
                        });
                        ui.end_row();
                    }

                    let status_bar_config = &mut self.visualizer_config.status_bar_config;

                    // Status bar padding
                    {
                        ui.label("Status bar padding: ");
                        ui.horizontal(|ui| {
                            for (value, text) in status_bar_config
                                .padding
                                .iter_mut()
                                .flat_map(|v| [&mut v.x, &mut v.y])
                                .zip(["Left", "Bottom", "Right", "Top"])
                            {
                                egui::DragValue::new(value)
                                    .range(0.0..=status_bar_config.em_size)
                                    .speed(0.01)
                                    .max_decimals(3)
                                    .ui(ui)
                                    .on_hover_text(text);
                            }
                        });
                        ui.end_row();
                    }

                    // Status bar font size
                    {
                        ui.label("Status bar font size: ");
                        egui::DragValue::new(&mut status_bar_config.em_size)
                            .range(0.0..=0.5)
                            .speed(0.01)
                            .max_decimals(3)
                            .ui(ui);
                        ui.end_row();
                    }

                    // progress bar height
                    {
                        let progress_bar_config = &mut self.visualizer_config.progress_bar_config;
                        ui.label("Progress bar height: ");
                        egui::DragValue::new(&mut progress_bar_config.height)
                            .range(0.0..=0.5)
                            .speed(0.01)
                            .max_decimals(3)
                            .ui(ui);
                        ui.end_row();
                    }

                    ui.label("");
                    ui.label("Unit: Ranim unit");
                    ui.end_row();
                }
            });
        });
    }

    fn output_settings_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("output_grid").show(ui, |ui| {
            // Resolution
            {
                ui.label("Resolution: ");
                let resolution = self.resolution();
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("resolution_combo")
                        .selected_text(format!(
                            "{}×{} ({})",
                            resolution.width,
                            resolution.height,
                            resolution.aspect_ratio_str()
                        ))
                        .show_ui(ui, |ui| {
                            // 16:9
                            ui.label(egui::RichText::new("16:9").strong());

                            let mut resolution_select_value =
                                |ui: &mut egui::Ui, selected_value: Resolution, text: &str| {
                                    let resolution = self.resolution();
                                    let mut resp =
                                        ui.selectable_label(resolution == selected_value, text);
                                    if resp.clicked() && resolution != selected_value {
                                        self.export_config.width = selected_value.width;
                                        self.export_config.height = selected_value.height;
                                        resp.mark_changed();
                                    }
                                    resp
                                };

                            resolution_select_value(ui, Resolution::HD, "1280×720 (HD)");
                            resolution_select_value(ui, Resolution::FHD, "1920×1080 (FHD)");
                            resolution_select_value(ui, Resolution::QHD, "2560×1440 (QHD)");
                            resolution_select_value(ui, Resolution::UHD, "3840×2160 (UHD)");
                            ui.separator();
                            // 16:10
                            ui.label(egui::RichText::new("16:10").strong());
                            resolution_select_value(ui, Resolution::WXGA, "1280×800 (WXGA)");
                            resolution_select_value(ui, Resolution::WUXGA, "1920×1200 (WUXGA)");
                            ui.separator();
                            // 4:3
                            ui.label(egui::RichText::new("4:3").strong());
                            resolution_select_value(ui, Resolution::SVGA, "800×600 (SVGA)");
                            resolution_select_value(ui, Resolution::XGA, "1024×768 (XGA)");
                            resolution_select_value(ui, Resolution::SXGA, "1280×960 (SXGA)");
                            ui.separator();
                            // 1:1
                            ui.label(egui::RichText::new("1:1").strong());
                            resolution_select_value(ui, Resolution::_1K_SQUARE, "1080×1080");
                            resolution_select_value(ui, Resolution::_2K_SQUARE, "2160×2160");
                            ui.separator();
                            // 21:9
                            ui.label(egui::RichText::new("21:9").strong());
                            resolution_select_value(ui, Resolution::UW_QHD, "3440×1440 (UW-QHD)");
                        });
                });
                ui.end_row();
                ui.label("");
                ui.horizontal(|ui| {
                    egui::DragValue::new(&mut self.export_config.width)
                        .update_while_editing(false)
                        .range(1..=7680)
                        .ui(ui)
                        .on_hover_text("Width (px)");
                    egui::DragValue::new(&mut self.export_config.height)
                        .update_while_editing(false)
                        .range(1..=4320)
                        .ui(ui)
                        .on_hover_text("Height (px)");
                });
            }
            ui.end_row();

            // FPS
            {
                let value = &mut self.export_config.fps;
                ui.label("Output FPS:");
                egui::DragValue::new(value)
                    .range(1u32..=240)
                    .update_while_editing(false)
                    .ui(ui);
            }
            ui.end_row();
        });
    }

    fn audio_settings_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("audio_grid").show(ui, |ui| {
            // Soundfont
            {
                ui.label("Soundfont:");
                ui.horizontal(|ui| {
                    if ui.button(egui_phosphor::regular::FOLDER_OPEN).clicked() {
                        self.show_load_soundfont_dialog();
                    }
                    match &self.soundfont_file {
                        Some(path) => {
                            ui.label(
                                path.file_name()
                                    .map(|u| u.display().to_string())
                                    .unwrap_or_else(|| "".to_string()),
                            );
                        }
                        None => {
                            ui.label("(None)");
                        }
                    }
                });
                ui.end_row();
            }

            // ui.label("Audio device:");
            // ui.end_row();

            // Sample rate
            {
                ui.label("Sample rate:");
                let value = &mut self.synth_settings.sample_rate;
                const COMMON_SAMPLE_RATES: [u32; 12] = [
                    8000, 11025, 16000, 22050, 44100, 48000, 88200, 96000, 176400, 192000, 352800,
                    384000,
                ];
                // [TODO] make this an editable combo box which supports custom input
                egui::ComboBox::from_id_salt("sample_rate_combo")
                    .selected_text(format!("{} Hz", value))
                    .show_ui(ui, |ui| {
                        for selected_value in COMMON_SAMPLE_RATES {
                            ui.selectable_value(
                                value,
                                selected_value,
                                format!("{} Hz", selected_value),
                            );
                        }
                    });
                ui.end_row();
            }

            // Block size
            {
                ui.label("Block size:");
                let value = &mut self.synth_settings.block_size;
                egui::DragValue::new(value)
                    .range(16..=1024)
                    .update_while_editing(false)
                    .ui(ui);
                ui.end_row();
            }

            // Maximum polyphony
            {
                ui.label("Max polyphony:");
                let value = &mut self.synth_settings.maximum_polyphony;
                egui::DragValue::new(value)
                    .range(1..=64)
                    .update_while_editing(false)
                    .ui(ui);
                ui.end_row();
            }

            // enable reverb and chorus
            {
                ui.label("");
                let value = &mut self.synth_settings.enable_reverb_and_chorus;
                ui.checkbox(value, "Enable reverb and chorus");
            }
        });
    }
}

impl MidiVisualizerApp {
    fn default_dock_state() -> egui_dock::DockState<MidiVisualizerTab> {
        use MidiVisualizerTab::*;
        let mut dock_state = egui_dock::DockState::new(vec![VideoPlayback]);
        let surface = dock_state.main_surface_mut();
        let [_, right_node] =
            surface.split_right(egui_dock::NodeIndex::root(), 0.625, vec![StyleSettings]);
        surface.split_below(right_node, 0.75, vec![OutputSettings]);
        dock_state
    }

    fn update_visible_tabs(&self) {
        let mut visible_tabs = self.visible_tabs.borrow_mut();
        visible_tabs.clear();
        visible_tabs.extend(
            self.dock_state
                .iter_all_tabs()
                .map(|(path, &tab)| (tab, path.node_path())),
        );
    }

    fn menu_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.menu_button(format!("{} File", egui_phosphor::regular::FILE), |ui| {
            if ui
                .button(format!("{} Open", egui_phosphor::regular::FOLDER_OPEN))
                .clicked()
            {
                self.show_open_dialog();
            }
            if ui
                .add_enabled(
                    !self.is_exporting(),
                    egui::Button::new(format!("{} Export", egui_phosphor::regular::EXPORT)),
                )
                .clicked()
            {
                self.show_export_dialog(ctx.clone());
            }
        });

        ui.menu_button(format!("{} View", egui_phosphor::regular::EYE), |ui| {
            let opened_tabs = self
                .dock_state
                .iter_all_tabs()
                .map(|(path, &tab)| (tab, path))
                .collect::<HashMap<_, _>>();

            for tab in MidiVisualizerTab::VARIANTS.iter().copied() {
                let path = opened_tabs.get(&tab).copied();
                let resp =
                    ui.selectable_label(path.is_some(), format!("{} {}", tab.icon(), tab.title()));
                if resp.clicked() {
                    if let Some(path) = path {
                        if self.is_closeable(&tab) {
                            self.dock_state.remove_tab(path);
                        }
                    } else {
                        self.dock_state.main_surface_mut().split_right(
                            egui_dock::NodeIndex::root(),
                            0.625,
                            vec![tab],
                        );
                    }
                }
            }
            ui.separator();
            if ui
                .button(format!(
                    "{} Revert to default",
                    egui_phosphor::regular::ERASER
                ))
                .clicked()
            {
                self.dock_state = Self::default_dock_state();
                self.update_visible_tabs();
            }
        });

        ui.menu_button(
            format!("{} Style", egui_phosphor::regular::PAINT_BRUSH),
            |ui| {
                // Save style
                if ui
                    .button(format!(
                        "{} Save style",
                        egui_phosphor::regular::FLOPPY_DISK
                    ))
                    .clicked()
                {
                    self.show_save_style_dialog();
                }

                // Load style
                if ui
                    .button(format!(
                        "{} Load style",
                        egui_phosphor::regular::FOLDER_OPEN
                    ))
                    .clicked()
                {
                    self.show_load_style_dialog();
                }

                ui.separator();

                // Revert style to default
                if ui
                    .button(format!(
                        "{} Revert to default",
                        egui_phosphor::regular::ERASER
                    ))
                    .clicked()
                {
                    self.show_revert_style_dialog();
                }
            },
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // light / dark mode toggle
            {
                let dark_mode = ui.visuals().dark_mode;
                let button_text = if dark_mode {
                    egui_phosphor::regular::SUN
                } else {
                    egui_phosphor::regular::MOON
                };
                let tooltip = if dark_mode {
                    "Switch to light mode"
                } else {
                    "Switch to dark mode"
                };
                if ui.button(button_text).on_hover_text(tooltip).clicked() {
                    if dark_mode {
                        ctx.set_theme(egui::Theme::Light);
                    } else {
                        ctx.set_theme(egui::Theme::Dark);
                    }
                }
            }
        });
    }
}

pub fn run_app(app: MidiVisualizerApp, #[cfg(target_arch = "wasm32")] container_id: String) {
    let build_app = |cc: &eframe::CreationContext| {
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);
        Ok(Box::new(app) as Box<dyn eframe::App>)
    };

    #[cfg(not(target_family = "wasm"))]
    {
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("Ranim Midi Visualizer")
                .with_inner_size([1280.0, 720.0]),
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        };

        // We need to clone title because run_native takes String (or &str) and app is moved into closure

        eframe::run_native("ranim_midi-visualizer", native_options, Box::new(build_app)).unwrap();
    }

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let web_options = eframe::WebOptions {
            ..Default::default()
        };

        // Handling canvas creation if not found to ensure compatibility
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document
            .get_element_by_id(&container_id)
            .and_then(|c| c.dyn_into::<web_sys::HtmlCanvasElement>().ok());

        let canvas = if let Some(canvas) = canvas {
            canvas
        } else {
            let canvas = document.create_element("canvas").unwrap();
            canvas.set_id(&container_id);
            document.body().unwrap().append_child(&canvas).unwrap();
            canvas.dyn_into::<web_sys::HtmlCanvasElement>().unwrap()
        };

        wasm_bindgen_futures::spawn_local(async {
            eframe::WebRunner::new()
                .start(canvas, web_options, Box::new(build_app))
                .await
                .expect("failed to start eframe");
        });
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
