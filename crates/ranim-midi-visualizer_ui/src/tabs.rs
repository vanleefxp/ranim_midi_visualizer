use enum_ordinalize::Ordinalize;

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
