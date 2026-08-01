use gpui::*;

use derive_more::{Deref, DerefMut, From, Into};
use jiff::SignedDuration;
use ranim::color::{self, AlphaColor, Srgb};

#[derive(Clone, Copy, From, Into, PartialEq, Debug, Deref, DerefMut)]
pub struct RanimColor(pub AlphaColor<Srgb>);

impl From<RanimColor> for Rgba {
    fn from(value: RanimColor) -> Self {
        let [r, g, b, a] = value.components;
        Rgba { r, g, b, a }
    }
}

impl From<RanimColor> for Hsla {
    fn from(value: RanimColor) -> Self {
        Rgba::from(value).into()
    }
}

impl From<Rgba> for RanimColor {
    fn from(value: Rgba) -> Self {
        let Rgba { r, g, b, a } = value;
        AlphaColor::new([r, g, b, a]).into()
    }
}

impl From<Hsla> for RanimColor {
    fn from(value: Hsla) -> Self {
        Rgba::from(value).into()
    }
}

pub fn rgba_to_string(rgba: impl Into<Rgba>) -> String {
    let color::Rgba8 { r, g, b, a } = RanimColor::from(rgba.into()).to_rgba8();
    format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a)
}

pub fn duration_to_string(duration: SignedDuration) -> String {
    let sec = duration.as_secs();
    let milli = duration.subsec_nanos() / 1_000_000;
    let (min, sec) = (sec.div_euclid(60), sec.rem_euclid(60));
    let (hour, min) = (min.div_euclid(60), min.rem_euclid(60));
    format!("{:02}:{:02}:{:02}.{:03}", hour, min, sec, milli)
}
