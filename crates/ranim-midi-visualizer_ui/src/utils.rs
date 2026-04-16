#![allow(unused)]

use eframe::egui;
use ranim::{
    color::{AlphaColor, Rgba8, Srgb},
    core::num::Integer as _,
    glam::{DVec2, dvec2},
};

pub fn to_ranim_dvec2(v: egui::Vec2) -> DVec2 {
    dvec2(v.x as f64, v.y as f64)
}

pub fn to_egui_vec2(v: DVec2) -> egui::Vec2 {
    egui::vec2(v.x as f32, v.y as f32)
}

pub fn to_ranim_color(color: egui::Color32) -> AlphaColor<Srgb> {
    let [r, g, b, a] = color.to_array();
    AlphaColor::from_rgba8(r, g, b, a)
}

pub fn to_egui_color(color: AlphaColor<Srgb>) -> egui::Color32 {
    let Rgba8 { r, g, b, a } = color.to_rgba8();
    egui::Color32::from_rgba_premultiplied(r, g, b, a)
}

pub fn nano_to_time_string(nano: u64) -> String {
    let micro = nano / 1000000;
    let (sec, micro) = micro.div_mod_floor(&1000);
    let (min, sec) = sec.div_mod_floor(&60);
    let (hour, min) = min.div_mod_floor(&60);

    format!("{:02}:{:02}:{:02}.{:03}", hour, min, sec, micro)
}
