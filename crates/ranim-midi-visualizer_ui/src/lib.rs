mod utils;
pub mod widgets;

use crate::{utils::nano_to_time_string, widgets::MidiVisualizerPreview};
use eframe::egui::{self, Widget};
use ranim::{SceneConfig, cmd::preview::Resolution};
use ranim_midi_visualizer_lib::MidiVisualizerConfig;
use std::{
    cell::{Ref, RefCell},
    ops::{Deref, DerefMut},
    path::PathBuf,
    time::{Duration, Instant},
};
use structured_midi::{MidiMusic, utils::func::LadderFn};

pub struct MidiVisualizerAppInner {
    status: AppStatus,
    /// the displaying MIDI music
    pub music: MidiMusic,
    /// configuration of the MIDI visualizer
    pub visualizer_config: MidiVisualizerConfig,
    /// configuration of the Ranim scene
    pub scene_config: SceneConfig,
    /// output video resolution
    pub resolution: Resolution,
    /// current playing time in nanoseconds
    pub time: u64,
    pub looping: bool,
    /// absolute time corresponding to the start of music
    ///
    /// When "play" button is clicked, this value is set to the instant of now minus the song's current playing time.
    pub play_start_t: Option<Instant>,

    /// whether the custom resolution dialog is open
    resolution_dialog_open: bool,
    /// time window for calculating NPS and legato index
    time_window: u64,
    /// total duration of the music
    duration: u64,
    /// export video framerate
    fps: u32,
    /// video playback speed
    playback_speed: f64,

    nps_max_cache: RefCell<Option<LadderFn<u64, f64>>>,
    note_count_cache: RefCell<Option<LadderFn<u64, usize>>>,
}

impl Default for MidiVisualizerAppInner {
    fn default() -> Self {
        Self {
            status: Default::default(),
            music: Default::default(),
            visualizer_config: Default::default(),
            scene_config: Default::default(),
            resolution: Resolution::FHD,
            time: 0,
            looping: false,
            play_start_t: None,

            resolution_dialog_open: false,
            time_window: 1_000_000_000, // 1 second
            duration: 0,
            fps: 60,
            playback_speed: 1.0,

            nps_max_cache: Default::default(),
            note_count_cache: Default::default(),
        }
    }
}

pub struct MidiVisualizerApp {
    inner: MidiVisualizerAppInner,
    dock_state: egui_dock::DockState<MidiVisualizerTab>,
}

impl Default for MidiVisualizerApp {
    fn default() -> Self {
        use MidiVisualizerTab::*;
        let dock_state = egui_dock::DockState::new(vec![VideoPlayback]);
        Self {
            inner: Default::default(),
            dock_state,
        }
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

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MidiVisualizerTab {
    VideoPlayback,
}

impl MidiVisualizerAppInner {
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

    pub fn toggle_play_pause(&mut self) {
        if self.is_playing() {
            self.pause();
        } else {
            self.play();
        }
    }

    pub fn step_frame(&mut self, n: isize) {
        // [TODO] when the division is not exact, there can be cumulative error
        let dt = 100_000_000 / self.fps as u64 * n.abs() as u64;
        if n >= 0 {
            self.time = (self.time + dt).max(self.duration);
        } else if self.time > dt {
            self.time -= dt;
        } else {
            self.time = 0;
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
        self.music = music;
        self.time = 0;
        self.duration = self.music.duration();
        self.clear_cache();
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

    fn clear_cache(&self) {
        self.nps_max_cache.take();
        self.note_count_cache.take();
    }
}

impl egui_dock::TabViewer for MidiVisualizerAppInner {
    type Tab = MidiVisualizerTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        use MidiVisualizerTab::*;
        match tab {
            VideoPlayback => "Video Playback",
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        use MidiVisualizerTab::*;
        match *tab {
            VideoPlayback => self.video_playback_ui(ui),
        }
    }

    fn scroll_bars(&self, tab: &Self::Tab) -> [bool; 2] {
        use MidiVisualizerTab::*;
        match tab {
            VideoPlayback => [false, false],
        }
    }

    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        use MidiVisualizerTab::*;
        match tab {
            VideoPlayback => false,
        }
    }
}

impl eframe::App for MidiVisualizerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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

        // resolution dialog
        if self.resolution_dialog_open {
            egui::Window::new("Resolution")
                .id(egui::Id::new("resolution_window"))
                .collapsible(false)
                .max_width(100.)
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        egui::Grid::new("resolution_grid")
                            .num_columns(1)
                            .show(ui, |ui| {
                                ui.label("Width:");
                                egui::DragValue::new(&mut self.resolution.width)
                                    .update_while_editing(false)
                                    .range(1..=10000)
                                    .ui(ui);
                                ui.end_row();
                                ui.label("Height:");
                                egui::DragValue::new(&mut self.resolution.height)
                                    .update_while_editing(false)
                                    .range(1..=10000)
                                    .ui(ui);
                            });
                        if ui.button("OK").clicked() {
                            self.resolution_dialog_open = false;
                        }
                    })
                });
        }

        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            egui::MenuBar::default().ui(ui, |ui| {
                ui.menu_button(format!("{} File", egui_phosphor::regular::FILE), |ui| {
                    if ui
                        .button(format!("{} Open", egui_phosphor::regular::FOLDER_OPEN))
                        .clicked()
                    {
                        let opened_file = rfd::FileDialog::new()
                            .add_filter("MIDI files", &["mid", "midi"])
                            .pick_file();
                        if let Some(path) = &opened_file {
                            // load music
                            if let Ok(src) = std::fs::read(path)
                                && let Ok(music) = MidiMusic::try_from(src.as_slice())
                            {
                                self.set_music(music);
                                self.status = AppStatus::FileOpened(path.clone());
                            } else {
                                self.status = AppStatus::ReadingFailed(path.clone());
                            }
                        }
                    }
                    if ui
                        .button(format!("{} Export", egui_phosphor::regular::EXPORT))
                        .clicked()
                    {
                        // [TODO] export video using Ranim
                    }
                });

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
                                ctx.set_visuals(egui::Visuals::light());
                            } else {
                                ctx.set_visuals(egui::Visuals::dark());
                            }
                        }
                    }
                });
            });
        });

        egui::Panel::bottom("bottom_panel").show_inside(ui, |ui| {
            // status message
            match &self.status {
                AppStatus::NoFileOpened => {
                    ui.label("Open a MIDI file to start visualization.");
                }
                AppStatus::FileOpened(path) => {
                    ui.horizontal(|ui| {
                        ui.label("Opened: ");
                        ui.code(path.display().to_string());
                    });
                }
                AppStatus::ReadingFailed(path) => {
                    ui.horizontal(|ui| {
                        ui.label("Failed to read: ");
                        ui.code(path.display().to_string());
                    });
                }
            }
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).inner_margin(0))
            .show_inside(ui, |ui| {
                egui_dock::DockArea::new(&mut self.dock_state)
                    .show_leaf_collapse_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .show_close_buttons(false)
                    .show_add_buttons(true)
                    .show_inside(ui, &mut self.inner);
            });
    }
}

impl MidiVisualizerAppInner {
    fn video_playback_ui(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().visuals.code_bg_color = egui::Color32::TRANSPARENT;

        egui::Panel::top("central_top").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                // resolution selector
                {
                    ui.label("Resolution: ");
                    let resolution = self.resolution;
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
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::HD,
                                "1280×720 (HD)",
                            );
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::FHD,
                                "1920×1080 (FHD)",
                            );
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::QHD,
                                "2560×1440 (QHD)",
                            );
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::UHD,
                                "3840×2160 (UHD)",
                            );
                            ui.separator();
                            // 16:10
                            ui.label(egui::RichText::new("16:10").strong());
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::WXGA,
                                "1280×800 (WXGA)",
                            );
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::WUXGA,
                                "1920×1200 (WUXGA)",
                            );
                            ui.separator();
                            // 4:3
                            ui.label(egui::RichText::new("4:3").strong());
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::SVGA,
                                "800×600 (SVGA)",
                            );
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::XGA,
                                "1024×768 (XGA)",
                            );
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::SXGA,
                                "1280×960 (SXGA)",
                            );
                            ui.separator();
                            // 1:1
                            ui.label(egui::RichText::new("1:1").strong());
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::_1K_SQUARE,
                                "1080×1080",
                            );
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::_2K_SQUARE,
                                "2160×2160",
                            );
                            ui.separator();
                            // 21:9
                            ui.label(egui::RichText::new("21:9").strong());
                            ui.selectable_value(
                                &mut self.resolution,
                                Resolution::UW_QHD,
                                "3440×1440 (UW-QHD)",
                            );
                            ui.separator();
                            if ui
                                .selectable_label(false, "Custom")
                                .on_hover_text("Open resolution dialog")
                                .clicked()
                            {
                                self.resolution_dialog_open = true;
                            }
                        });
                }
                ui.spacing();

                // time window edit
                {
                    ui.label("Time window (s):");
                    let resp = egui::DragValue::new(&mut self.time_window)
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
                }
                ui.spacing();

                // FPS edit
                {
                    ui.label("Output FPS:");
                    egui::DragValue::new(&mut self.fps)
                        .range(1u32..=400)
                        .update_while_editing(false)
                        .ui(ui);
                }
                ui.spacing();
            });
        });

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
                        && let Some(start_t) = &mut self.play_start_t
                    {
                        *start_t = Instant::now()
                            - Duration::from_nanos((self.time as f64 / self.playback_speed) as u64);
                    }
                }

                ui.separator();

                // time display
                ui.code(nano_to_time_string(self.time));

                // time slider
                {
                    ui.style_mut().spacing.slider_width = ui.available_width();
                    let resp = egui::Slider::new(&mut self.time, 0..=self.duration)
                        .show_value(false)
                        .handle_shape(egui::style::HandleShape::Circle)
                        .ui(ui);
                    if resp.changed() && self.is_playing() {
                        self.play();
                    }
                }
            });
        });

        // Preview area
        egui::CentralPanel::default()
            .show_inside(ui, |ui| {
                let mut preview_widget = MidiVisualizerPreview::new(
                    &self.music,
                    &self.visualizer_config,
                    &self.scene_config,
                    self.resolution,
                    self.time_window,
                );
                preview_widget.time = self.time;

                let cache = &mut preview_widget.cache;
                cache.note_count = Some(self.note_count_fn()(&self.time));
                cache.note_count_total = Some(self.note_count_total());
                cache.nps_max = Some(self.nps_max_fn()(&self.time));

                preview_widget.ui(ui);
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
