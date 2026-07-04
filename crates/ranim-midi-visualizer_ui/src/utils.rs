#![allow(unused)]

use std::{
    collections::HashMap,
    num::{NonZero, NonZeroU64},
    ops::Range,
};

use derive_more::{AsMut, AsRef, Deref, DerefMut, From, Into};
use eframe::egui;
use ranim::{
    color::{AlphaColor, Rgba8, Srgb},
    core::{components::width::Width, num::Integer as _},
    glam::{DVec2, dvec2},
    items::vitem::text::{
        FontStretch, FontStyle as RanimFontStyle, FontVariant as RanimFontVariant, FontWeight,
        TextFont as RanimTextFont,
    },
};
use ranim_midi_visualizer_lib::config::{
    ColorBy, MetricBase, MidiVisualizerConfig as RanimMidiVisualizerConfig,
    NoteConfig as RanimNoteConfig, ProgressBarConfig as RanimProgressBarConfig,
    StatusBarConfig as RanimStatusBarConfig,
};
use ranim_music::items::{
    PianoKeyboardColor as RanimPianoKeyboardColor, PianoKeyboardConfig as RanimPianoKeyboardConfig,
    PianoKeyboardSize,
};
use typed_floats::tf64;

#[derive(Debug, Clone, Copy, Deref, DerefMut, AsRef, AsMut, From, Into)]
pub struct RanimColor(pub AlphaColor<Srgb>);

#[derive(Debug, Clone, Copy, Deref, DerefMut, AsRef, AsMut, From, Into)]
pub struct EguiColor(pub egui::Color32);

impl From<RanimColor> for egui::Color32 {
    fn from(value: RanimColor) -> Self {
        let Rgba8 { r, g, b, a } = value.to_rgba8();
        egui::Color32::from_rgba_unmultiplied(r, g, b, a)
    }
}

impl From<EguiColor> for AlphaColor<Srgb> {
    fn from(value: EguiColor) -> Self {
        let [r, g, b, a] = value.to_array();
        AlphaColor::from_rgba8(r, g, b, a)
    }
}

pub fn to_ranim_dvec2(v: egui::Vec2) -> DVec2 {
    dvec2(v.x as f64, v.y as f64)
}

pub fn to_egui_vec2(v: DVec2) -> egui::Vec2 {
    egui::vec2(v.x as f32, v.y as f32)
}

pub fn to_ranim_color(color: egui::Color32) -> AlphaColor<Srgb> {
    EguiColor(color).into()
}

pub fn to_egui_color(color: AlphaColor<Srgb>) -> egui::Color32 {
    RanimColor(color).into()
}

pub fn nano_to_time_string(nano: i64) -> String {
    let micro = nano / 1000000;
    let (sec, micro) = micro.div_mod_floor(&1000);
    let (min, sec) = sec.div_mod_floor(&60);
    let (hour, min) = min.div_mod_floor(&60);

    format!("{:02}:{:02}:{:02}.{:03}", hour, min, sec, micro)
}

pub fn egui_color_to_hex_string(color: egui::Color32) -> String {
    let [r, g, b, a] = color.to_array();
    format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a)
}

pub fn ranim_color_to_hex_string(color: AlphaColor<Srgb>) -> String {
    let Rgba8 { r, g, b, a } = color.to_rgba8();
    format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PianoKeyboardColor {
    /// Fill color of black and white keys.
    pub key_color: [egui::Color32; 2],
    /// Stroke color of keys.
    pub stroke_color: egui::Color32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PianoKeyboardConfig {
    /// Size of the piano keyboard.
    pub size: PianoKeyboardSize,
    /// Color of the piano keyboard.
    pub color: PianoKeyboardColor,
    /// Range of keys to be displayed.
    pub key_range: Range<i8>,
    /// Stroke width of keys.
    pub stroke_width: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatusBarConfig {
    /// font size unit
    pub em_size: f64,
    /// bottom-left and top-right paddings
    pub padding: [DVec2; 2],
    /// background color
    pub bg_color: egui::Color32,
    /// text color
    pub fg_color: egui::Color32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProgressBarConfig {
    /// height of the progress bar
    pub height: f64,
    /// foreground color
    pub fg_color: egui::Color32,
    /// background color
    pub bg_color: egui::Color32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MidiVisualizerConfig {
    pub metric_base: MetricBase,
    pub note_config: NoteConfig,
    pub scroll_speed: f64,
    pub buf_time: [u64; 2],
    pub keyboard_config: PianoKeyboardConfig,
    pub status_bar_config: StatusBarConfig,
    pub progress_bar_config: ProgressBarConfig,
    pub time_window: NonZeroU64,
    #[serde(skip)] // [TODO] make this field savable
    pub text_font: RanimTextFont,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct FontVariant {
    style: FontStyle,
    weight: u16,
    stretch: u16,
}

impl From<RanimPianoKeyboardColor> for PianoKeyboardColor {
    fn from(value: RanimPianoKeyboardColor) -> Self {
        let RanimPianoKeyboardColor {
            key_color,
            stroke_color,
        } = value;
        PianoKeyboardColor {
            key_color: key_color.map(to_egui_color),
            stroke_color: to_egui_color(stroke_color),
        }
    }
}

impl From<PianoKeyboardColor> for RanimPianoKeyboardColor {
    fn from(value: PianoKeyboardColor) -> Self {
        let PianoKeyboardColor {
            key_color,
            stroke_color,
        } = value;
        RanimPianoKeyboardColor {
            key_color: key_color.map(to_ranim_color),
            stroke_color: to_ranim_color(stroke_color),
        }
    }
}

impl From<RanimPianoKeyboardConfig> for PianoKeyboardConfig {
    fn from(value: RanimPianoKeyboardConfig) -> Self {
        let RanimPianoKeyboardConfig {
            size,
            color,
            key_range,
            stroke_width,
        } = value;
        PianoKeyboardConfig {
            size,
            color: color.into(),
            key_range,
            stroke_width: stroke_width.0,
        }
    }
}

impl From<PianoKeyboardConfig> for RanimPianoKeyboardConfig {
    fn from(value: PianoKeyboardConfig) -> Self {
        let PianoKeyboardConfig {
            size,
            color,
            key_range,
            stroke_width,
        } = value;
        RanimPianoKeyboardConfig {
            size,
            color: color.into(),
            key_range,
            stroke_width: Width(stroke_width),
        }
    }
}

impl PianoKeyboardConfig {
    pub fn width_range(&self, clip_black: bool) -> Range<f64> {
        self.size.width_range(&self.key_range, clip_black)
    }
}

impl From<RanimStatusBarConfig> for StatusBarConfig {
    fn from(value: RanimStatusBarConfig) -> Self {
        let RanimStatusBarConfig {
            em_size,
            padding,
            bg_color,
            fg_color,
        } = value;
        StatusBarConfig {
            em_size,
            padding,
            bg_color: to_egui_color(bg_color),
            fg_color: to_egui_color(fg_color),
        }
    }
}

impl From<StatusBarConfig> for RanimStatusBarConfig {
    fn from(value: StatusBarConfig) -> Self {
        let StatusBarConfig {
            em_size,
            padding,
            bg_color,
            fg_color,
        } = value;
        RanimStatusBarConfig {
            em_size,
            padding,
            bg_color: to_ranim_color(bg_color),
            fg_color: to_ranim_color(fg_color),
        }
    }
}

impl StatusBarConfig {
    pub fn height(&self) -> f64 {
        self.em_size + self.padding[0].y + self.padding[1].y
    }
}

impl From<RanimProgressBarConfig> for ProgressBarConfig {
    fn from(value: RanimProgressBarConfig) -> Self {
        let RanimProgressBarConfig {
            height,
            fg_color,
            bg_color,
        } = value;
        ProgressBarConfig {
            height,
            fg_color: to_egui_color(fg_color),
            bg_color: to_egui_color(bg_color),
        }
    }
}

impl From<ProgressBarConfig> for RanimProgressBarConfig {
    fn from(value: ProgressBarConfig) -> Self {
        let ProgressBarConfig {
            height,
            fg_color,
            bg_color,
        } = value;
        RanimProgressBarConfig {
            height,
            fg_color: to_ranim_color(fg_color),
            bg_color: to_ranim_color(bg_color),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NoteConfig {
    pub colors: Vec<egui::Color32>,
    pub color_by: ColorBy,
    pub h_scale: [f64; 2],
}

impl From<RanimNoteConfig> for NoteConfig {
    fn from(value: RanimNoteConfig) -> Self {
        let RanimNoteConfig {
            colors,
            color_by,
            h_scale,
        } = value;
        NoteConfig {
            colors: colors.into_iter().map(to_egui_color).collect(),
            color_by,
            h_scale: h_scale.map(f64::from),
        }
    }
}

impl From<NoteConfig> for RanimNoteConfig {
    fn from(value: NoteConfig) -> Self {
        let NoteConfig {
            colors,
            color_by,
            h_scale,
        } = value;
        RanimNoteConfig {
            colors: colors.into_iter().map(to_ranim_color).collect(),
            color_by,
            h_scale: h_scale.map(|v| v.try_into().unwrap()),
        }
    }
}

impl From<RanimMidiVisualizerConfig> for MidiVisualizerConfig {
    fn from(value: RanimMidiVisualizerConfig) -> Self {
        let RanimMidiVisualizerConfig {
            metric_base,
            note_config,
            scroll_speed,
            buf_time,
            keyboard_config,
            status_bar_config,
            progress_bar_config,
            time_window,
            text_font,
        } = value;
        MidiVisualizerConfig {
            metric_base,
            scroll_speed: scroll_speed.into(),
            note_config: note_config.into(),
            buf_time: buf_time.map(|v| (f64::from(v) * 1e9) as u64),
            keyboard_config: keyboard_config.into(),
            status_bar_config: status_bar_config.into(),
            progress_bar_config: progress_bar_config.into(),
            time_window: NonZeroU64::try_from((f64::from(time_window) * 1e9) as u64).unwrap(),
            text_font,
        }
    }
}

impl From<MidiVisualizerConfig> for RanimMidiVisualizerConfig {
    fn from(value: MidiVisualizerConfig) -> Self {
        let MidiVisualizerConfig {
            metric_base,
            note_config,
            scroll_speed,
            buf_time,
            keyboard_config,
            status_bar_config,
            progress_bar_config,
            time_window,
            text_font,
        } = value;
        RanimMidiVisualizerConfig {
            metric_base,
            note_config: note_config.into(),
            scroll_speed: scroll_speed.try_into().unwrap(),
            buf_time: buf_time.map(|v| tf64::PositiveFinite::try_from(v as f64 / 1e9).unwrap()),
            keyboard_config: keyboard_config.into(),
            status_bar_config: status_bar_config.into(),
            progress_bar_config: progress_bar_config.into(),
            time_window: tf64::StrictlyPositive::try_from(time_window.get() as f64 / 1e9).unwrap(),
            text_font,
        }
    }
}

impl Default for MidiVisualizerConfig {
    fn default() -> Self {
        Self::from(RanimMidiVisualizerConfig::default())
    }
}

impl From<RanimFontStyle> for FontStyle {
    fn from(value: RanimFontStyle) -> Self {
        match value {
            RanimFontStyle::Normal => FontStyle::Normal,
            RanimFontStyle::Italic => FontStyle::Italic,
            RanimFontStyle::Oblique => FontStyle::Oblique,
        }
    }
}

impl From<FontStyle> for RanimFontStyle {
    fn from(value: FontStyle) -> Self {
        match value {
            FontStyle::Normal => RanimFontStyle::Normal,
            FontStyle::Italic => RanimFontStyle::Italic,
            FontStyle::Oblique => RanimFontStyle::Oblique,
        }
    }
}

impl From<RanimFontVariant> for FontVariant {
    fn from(value: RanimFontVariant) -> Self {
        let RanimFontVariant {
            style,
            weight,
            stretch,
        } = value;
        let style = style.into();
        let weight = weight.to_number();
        let stretch = (stretch.to_ratio().get() * 1000.) as u16;
        FontVariant {
            style,
            weight,
            stretch,
        }
    }
}

impl From<FontVariant> for RanimFontVariant {
    fn from(value: FontVariant) -> Self {
        let FontVariant {
            style,
            weight,
            stretch,
        } = value;
        let style = style.into();
        let weight = FontWeight::from_number(weight);
        let stretch = FontStretch::from_number(stretch);
        RanimFontVariant {
            style,
            weight,
            stretch,
        }
    }
}
