use ranim::{
    core::animation::Eval,
    glam::{DVec2, DVec3, dvec2, dvec3},
    items::vitem::geometry::Rectangle,
    prelude::{FillColor, StrokeColor, With},
};

use crate::stroke_and_fill::StrokeAndFill;

pub struct MidiNoteAnim {
    pub origin: DVec3,
    pub scroll_size: DVec2,
    pub note_height: f64,
    pub stroke_and_fill: StrokeAndFill,
}

impl Eval<Rectangle> for MidiNoteAnim {
    fn eval_alpha(&self, alpha: f64) -> Rectangle {
        let &Self {
            origin,
            scroll_size,
            note_height,
            stroke_and_fill,
        } = self;
        let StrokeAndFill {
            fill_rgba,
            stroke_rgba,
            ..
        } = stroke_and_fill;
        let DVec2 {
            x: width,
            y: scroll_height,
        } = scroll_size;
        let t1 = note_height / (note_height + scroll_height);
        let t2 = scroll_height / (note_height + scroll_height);
        let (p0, size) = if note_height > scroll_height {
            // 0 <= t2 < t1 <= 1
            if alpha < t2 {
                let t = alpha / t2;
                let p0 = origin + dvec3(0., scroll_height * (1. - t), 0.);
                let size = dvec2(width, scroll_height * t);
                (p0, size)
            } else if alpha < t1 {
                (origin, scroll_size)
            } else {
                let t = (alpha - t1) / (1. - t1);
                let size = dvec2(width, scroll_height * (1. - t));
                (origin, size)
            }
        } else {
            // 0 <= t1 < t2 <= 1
            if alpha < t1 {
                let t = alpha / t1;
                let p0 = origin + dvec3(0., scroll_height - note_height * t, 0.);
                let size = dvec2(width, note_height * t);
                (p0, size)
            } else if alpha < t2 {
                let t = (alpha - t1) / (t2 - t1);
                let p0 = origin + dvec3(0., (scroll_height - note_height) * (1. - t), 0.);
                let size = dvec2(width, note_height);
                (p0, size)
            } else {
                let t = (alpha - t2) / (1. - t2);
                let size = dvec2(width, note_height * (1. - t));
                (origin, size)
            }
        };
        Rectangle::from_min_size(p0, size).with(|item| {
            item.set_fill_color(fill_rgba).set_stroke_color(stroke_rgba);
        })
    }
}
