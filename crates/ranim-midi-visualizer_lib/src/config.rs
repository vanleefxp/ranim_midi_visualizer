use std::sync::LazyLock;

use derivative::Derivative;
use ranim::{
    color::{AlphaColor, Srgb, rgb8},
    glam::{DVec2, dvec2},
    items::vitem::text::TextFont,
};
use ranim_music::items::PianoKeyboardConfig;
use typed_floats::tf64;

pub static DEFAULT_TEXT_FONT: LazyLock<TextFont> = LazyLock::new(|| {
    TextFont::new([
        "Maple Mono NF",
        "Cascadia Code NF",
        "LXGW WenKai Mono",
        "Consolas",
        "Monaco",
        "Courier New",
    ])
});
pub const DEFAULT_NOTE_COLORS: &[AlphaColor<Srgb>] = &[
    rgb8(0x89, 0xb9, 0xeb),
    rgb8(0x9b, 0xe3, 0x47),
    rgb8(0xf7, 0x93, 0x1e),
    rgb8(0xf7, 0xc7, 0x1e),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ColorBy {
    #[default]
    Voice,
    Staff,
    KeyColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MetricBase {
    /// Metric in seconds
    #[default]
    Time,
    /// Metric in beats
    Beat,
}

/// Configuration for the bottom status bar displaying data.
#[derive(Derivative)]
#[derivative(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StatusBarConfig {
    /// font size unit
    #[derivative(Default(value = "0.2"))]
    pub em_size: f64,
    /// bottom-left and top-right paddings
    #[derivative(Default(value = "[dvec2(0.1, 0.1), dvec2(0.1, 0.05)]"))]
    pub padding: [DVec2; 2],
    /// background color
    #[derivative(Default(value = "AlphaColor::BLACK.with_alpha(0.9)"))] // rgba(0, 0, 0, 0.9)
    pub bg_color: AlphaColor<Srgb>,
    /// text color
    #[derivative(Default(value = "AlphaColor::WHITE"))]
    pub fg_color: AlphaColor<Srgb>,
}

impl StatusBarConfig {
    /// Returns the height of the status bar. Equals to the sum of top padding, bottom padding, and font em-size.
    pub fn height(&self) -> f64 {
        self.em_size + self.padding[0].y + self.padding[1].y
    }
}

/// Top progress bar displaying the current time position in the song.
#[derive(Derivative)]
#[derivative(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProgressBarConfig {
    /// progress bar height
    #[derivative(Default(value = "0.06"))]
    pub height: f64,
    /// progress bar foreground color
    #[derivative(Default(value = "rgb8(168, 163, 204)"))] // rgb(168, 163, 204)
    pub fg_color: AlphaColor<Srgb>,
    /// progress bar background color
    #[derivative(Default(value = "AlphaColor::TRANSPARENT"))]
    pub bg_color: AlphaColor<Srgb>,
}

#[derive(Derivative)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derivative(Clone, Debug, Default)]
pub struct NoteConfig {
    #[derivative(Default(value = "DEFAULT_NOTE_COLORS.to_vec()"))]
    pub colors: Vec<AlphaColor<Srgb>>,
    #[derivative(Default(value = "ColorBy::Voice"))]
    pub color_by: ColorBy,
    #[derivative(Default(value = "[0.8, 1.].map(|v| v.try_into().unwrap())"))]
    pub h_scale: [tf64::PositiveFinite; 2],
}

#[derive(Derivative)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derivative(Clone, Debug, Default)]
// #[non_exhaustive]
pub struct MidiVisualizerConfig {
    pub metric_base: MetricBase,
    #[derivative(Default(value = "2.0.try_into().unwrap()"))]
    pub scroll_speed: tf64::StrictlyPositiveFinite,
    #[derivative(Default(value = "[2.0, 2.0].map(|v| v.try_into().unwrap())"))]
    pub buf_time: [tf64::PositiveFinite; 2],
    pub note_config: NoteConfig,
    pub keyboard_config: PianoKeyboardConfig,
    pub status_bar_config: StatusBarConfig,
    pub progress_bar_config: ProgressBarConfig,
    #[derivative(Default(value = "1.0.try_into().unwrap()"))]
    pub time_window: tf64::StrictlyPositive,
    #[serde(skip)]
    #[derivative(Default(value = "DEFAULT_TEXT_FONT.clone()"))]
    pub text_font: TextFont,
}
