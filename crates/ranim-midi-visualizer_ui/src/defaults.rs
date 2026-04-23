use eframe::egui;

use crate::{MidiVisualizerApp, MidiVisualizerAppInner2, tabs::MidiVisualizerTab};

impl Default for MidiVisualizerAppInner2 {
    fn default() -> Self {
        Self {
            midi_file: None,
            music: Default::default(),
            synth: None,
            audio_device_idx: 0,

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

impl MidiVisualizerApp {
    pub(crate) fn default_dock_state() -> egui_dock::DockState<MidiVisualizerTab> {
        use MidiVisualizerTab::*;
        let mut dock_state = egui_dock::DockState::new(vec![VideoPlayback]);
        let surface = dock_state.main_surface_mut();
        let [_, right_node] =
            surface.split_right(egui_dock::NodeIndex::root(), 0.625, vec![StyleSettings]);
        surface.split_below(right_node, 0.75, vec![OutputSettings]);
        dock_state
    }
}
