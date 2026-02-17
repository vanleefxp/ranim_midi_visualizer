use std::{
    array,
    cell::{Ref, RefCell},
    collections::HashMap,
    ops::Range,
};

use glam::{DVec2, DVec3, Vec3Swizzles as _, dvec2, dvec3};
use ranim::{
    anims::morph::MorphAnim,
    color::{AlphaColor, Srgb, palettes::manim},
    core::{Extract, core_item::CoreItem, timeline::Timeline},
    items::vitem::{
        VItem,
        geometry::{Rectangle, anchor::Origin},
    },
    prelude::*,
    utils::{bezier::PathBuilder, rate_functions::linear},
};

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct PianoKeyboardSize {
    pub white_size: DVec2,
    pub black_size: DVec2,
    pub corner_size: DVec2,
    pub black_offset: [f64; 5],
    pub note_h_scale: (f64, f64),
}

impl Interpolatable for PianoKeyboardSize {
    fn lerp(&self, target: &Self, t: f64) -> Self {
        Self {
            white_size: self.white_size.lerp(target.white_size, t),
            black_size: self.black_size.lerp(target.black_size, t),
            corner_size: self.corner_size.lerp(target.corner_size, t),
            black_offset: array::from_fn(|i| self.black_offset[i].lerp(&target.black_offset[i], t)),
            note_h_scale: (
                self.note_h_scale.0.lerp(&target.note_h_scale.0, t),
                self.note_h_scale.1.lerp(&target.note_h_scale.1, t),
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PianoKeyboard {
    origin: DVec3,
    size: PianoKeyboardSize,
    key_range: Range<u8>,
    highlighted_keys: HashMap<u8, AlphaColor<Srgb>>,
    keys: RefCell<Option<Vec<VItem>>>,
}

impl Locate<PianoKeyboard> for Origin {
    fn locate(&self, target: &PianoKeyboard) -> DVec3 {
        target.origin
    }
}

impl Default for PianoKeyboardSize {
    fn default() -> Self {
        Self {
            white_size: dvec2(0.35, 2.),
            black_size: dvec2(0.225, 1.1),
            corner_size: dvec2(0.08, 0.08),
            black_offset: [-0.1, 0.1, -0.1, 0.0, 0.1],
            note_h_scale: (0.8, 1.),
        }
    }
}

impl Default for PianoKeyboard {
    fn default() -> Self {
        Self {
            origin: DVec3::ZERO,
            size: Default::default(),
            key_range: 21..109, // standard piano keyboard
            highlighted_keys: Default::default(),
            keys: Default::default(),
        }
    }
}

impl Aabb for PianoKeyboard {
    fn aabb(&self) -> [DVec3; 2] {
        self.keys().aabb()
    }
}

impl ShiftTransform for PianoKeyboard {
    fn shift(&mut self, shift: DVec3) -> &mut Self {
        self.origin.shift(shift);
        if let Some(keys) = self.keys.borrow_mut().as_mut() {
            keys.shift(shift);
        }
        self
    }
}

impl PianoKeyboard {
    pub fn new(key_range: &Range<u8>) -> Self {
        Self {
            key_range: key_range.clone(),
            ..Default::default()
        }
    }

    pub fn size(&self) -> &PianoKeyboardSize {
        &self.size
    }

    pub fn set_size(&mut self, f: impl FnOnce(&mut PianoKeyboardSize)) -> &mut Self {
        let orig_size = self.size.clone();
        f(&mut self.size);
        if self.size != orig_size {
            self.keys.borrow_mut().take();
        }
        self
    }

    pub fn set_key_range(&mut self, key_range: Range<u8>) -> &mut Self {
        let orig_range = self.key_range.clone();
        self.key_range = key_range.clone();
        if self.key_range != orig_range {
            self.keys.borrow_mut().take();
        }
        self
    }

    pub fn highlighted_keys(&self) -> &HashMap<u8, AlphaColor<Srgb>> {
        &self.highlighted_keys
    }

    pub fn highlight_keys(
        &mut self,
        f: impl FnOnce(&mut HashMap<u8, AlphaColor<Srgb>>),
    ) -> &mut Self {
        let orig_keys = self.highlighted_keys.clone();
        f(&mut self.highlighted_keys);
        if self.highlighted_keys != orig_keys {
            self.set_key_colors();
        }
        self
    }

    pub fn keys<'a>(&'a self) -> Ref<'a, Vec<VItem>> {
        if self.keys.borrow().is_none() {
            let keys = self.generate_keys();
            self.keys.replace(Some(keys));
            self.set_key_colors();
        }
        Ref::map(self.keys.borrow(), |v| {
            v.as_ref().expect("`keys` can't be `None`")
        })
    }

    pub fn key_origin(&self, tone: u8) -> DVec3 {
        let (octave, otone) = (tone / 12, tone % 12);

        let black_width = self.size.black_size.x;
        let white_width = self.size.white_size.x;
        let black_offset = self.size.black_offset;

        let disp = match otone {
            0 => 0.,
            1 => white_width + black_width * (black_offset[0] - 0.5),
            2 => white_width,
            3 => white_width * 2. + black_width * (black_offset[1] - 0.5),
            4 => white_width * 2.,
            5 => white_width * 3.,
            6 => white_width * 4. + black_width * (black_offset[2] - 0.5),
            7 => white_width * 4.,
            8 => white_width * 5. + black_width * (black_offset[3] - 0.5),
            9 => white_width * 5.,
            10 => white_width * 6. + black_width * (black_offset[4] - 0.5),
            11 => white_width * 6.,
            _ => unreachable!(),
        };

        self.origin + dvec3(disp + (octave as f64 - 5.) * white_width * 7., 0., 0.)
    }

    pub fn anim_note(
        &self,
        tl: &mut Timeline,
        note_setup: impl Fn(&mut Rectangle),
        tone: u8,
        duration: f64,
        scroll_speed: f64,
        // note_height: f64,
        scroll_height: f64,
    ) {
        let (key_width, h_scale) = match tone % 12 {
            1 | 3 | 6 | 8 | 10 => (self.size.black_size.x, self.size.note_h_scale.1),
            _ => (self.size.white_size.x, self.size.note_h_scale.0),
        };
        let origin = self.key_origin(tone) + ((1. - h_scale) * key_width * 0.5) * DVec3::X;
        let top_left = origin + DVec3::Y * scroll_height;
        let note_width = key_width * h_scale;
        let note_height = scroll_speed * duration;
        let scroll_time = scroll_height / scroll_speed;

        // starts from nothing
        let mut note =
            Rectangle::from_min_size(top_left, dvec2(note_width, 0.)).with(|item| note_setup(item));
        if note_height > scroll_height {
            // fills the scroll height
            let note2 = Rectangle::from_min_size(origin, dvec2(note_width, scroll_height))
                .with(|item| note_setup(item));
            // ends at nothing
            let note3 = Rectangle::from_min_size(origin, dvec2(note_width, 0.))
                .with(|item| note_setup(item));
            tl.play(
                note.morph_to(note2)
                    .with_duration(scroll_time)
                    .with_rate_func(linear),
            )
            .forward(duration - scroll_time)
            .play(
                note.morph_to(note3)
                    .with_duration(scroll_time)
                    .with_rate_func(linear),
            );
        } else {
            let note2_bottom_left = origin + DVec3::Y * (scroll_height - note_height);
            let note_size = dvec2(note_width, note_height);
            let note2 = Rectangle::from_min_size(note2_bottom_left, note_size)
                .with(|item| note_setup(item));
            let note3 = Rectangle::from_min_size(origin, note_size).with(|item| note_setup(item));
            let note4 = Rectangle::from_min_size(origin, dvec2(note_width, 0.))
                .with(|item| note_setup(item));
            tl.play(
                note.morph_to(note2)
                    .with_duration(duration)
                    .with_rate_func(linear),
            )
            .play(
                note.morph_to(note3)
                    .with_duration(scroll_time - duration)
                    .with_rate_func(linear),
            )
            .play(
                note.morph_to(note4)
                    .with_duration(duration)
                    .with_rate_func(linear),
            );
        }
    }

    fn generate_keys(&self) -> Vec<VItem> {
        let u = DVec3::X;
        let w = DVec3::Z;

        let PianoKeyboardSize {
            white_size,
            black_size,
            corner_size,
            black_offset,
            ..
        } = self.size;

        let i_white_key = piano_key(white_size, corner_size).with(|item| {
            item.set_fill_color(manim::WHITE)
                .set_stroke_color(manim::GREY_C)
                .set_stroke_width(0.005);
        });
        let i_black_key = piano_key(black_size, corner_size).with(|item| {
            item.set_fill_color(manim::BLACK)
                .set_stroke_color(manim::GREY_C)
                .set_stroke_width(0.005);
        });
        let mut i_octave: [VItem; 12] = array::from_fn(|_| VItem::empty());

        for (step, &tone) in [0, 2, 4, 5, 7, 9, 11].iter().enumerate() {
            i_octave[tone] = i_white_key.clone().with(|item| {
                item.shift(u * (white_size.x * step as f64));
            });
        }
        for ((&step, &tone), &dx) in [0, 1, 3, 4, 5]
            .iter()
            .zip([1, 3, 6, 8, 10].iter())
            .zip(black_offset.iter())
        {
            i_octave[tone] = i_black_key.clone().with(|item| {
                item.shift(
                    u * (white_size.x * (step + 1) as f64 + black_size.x * (dx - 0.5)) + w * 0.001,
                );
            });
        }

        let Range {
            start: k_start,
            end: k_end,
        } = self.key_range;
        let Range {
            start: o_start,
            end: o_end,
        } = octave_range(&self.key_range);
        let mut keys = Vec::with_capacity(self.key_range.len());

        if o_end < o_start {
            for i in (k_start - o_end * 12)..(k_end - o_end * 12) {
                keys.push(i_octave[i as usize].clone().with(|item| {
                    item.shift((o_end as f64 - 5.) * 7. * white_size.x * u)
                        .shift(self.origin);
                }));
            }
        } else {
            for i in (k_start - (o_start - 1) * 12)..12 {
                keys.push(i_octave[i as usize].clone().with(|item| {
                    item.shift((o_start as f64 - 6.) * 7. * white_size.x * u)
                        .shift(self.origin);
                }));
            }
            for o in o_start..o_end {
                keys.extend(i_octave.clone().with(|item| {
                    item.shift((o as f64 - 5.) * 7. * white_size.x * u)
                        .shift(self.origin);
                }));
            }
            for i in 0..(k_end - o_end * 12) {
                keys.push(i_octave[i as usize].clone().with(|item| {
                    item.shift((o_end as f64 - 5.) * 7. * white_size.x * u)
                        .shift(self.origin);
                }));
            }
        }

        keys
    }

    fn set_key_colors(&self) {
        if let Some(keys) = self.keys.borrow_mut().as_mut() {
            self.key_range
                .clone()
                .zip(keys.iter_mut())
                .for_each(|(tone, key)| match tone % 12 {
                    1 | 3 | 6 | 8 | 10 => {
                        key.set_fill_color(manim::BLACK);
                    }
                    _ => {
                        key.set_fill_color(manim::WHITE);
                    }
                });
            self.highlighted_keys.iter().for_each(|(&tone, &color)| {
                let idx = tone - self.key_range.start;
                if let Some(key) = keys.get_mut(idx as usize) {
                    match tone % 12 {
                        1 | 3 | 6 | 8 | 10 => {
                            key.set_fill_color(color.map_lightness(|x| x - 0.2));
                        }
                        _ => {
                            key.set_fill_color(color);
                        }
                    }
                }
            });
        }
    }

    fn scale_size(&mut self, scale: DVec3) -> &mut Self {
        let cfg = &mut self.size;
        cfg.white_size *= scale.xy();
        cfg.black_size *= scale.xy();
        cfg.corner_size *= scale.xy();
        self
    }

    fn transform_items(&self, transformation: impl FnOnce(&mut Vec<VItem>)) {
        if let Some(keys) = self.keys.borrow_mut().as_mut() {
            transformation(keys)
        }
    }
}

impl ScaleTransform for PianoKeyboard {
    fn scale(&mut self, scale: DVec3) -> &mut Self {
        self.scale_size(scale);
        self.origin.scale(scale);
        self.transform_items(|item| item.scale(scale).discard());
        self
    }
}

impl Into<Vec<VItem>> for PianoKeyboard {
    fn into(self) -> Vec<VItem> {
        self.keys().clone()
    }
}

fn piano_key(key_size: DVec2, corner_size: DVec2) -> VItem {
    let u = DVec3::X;
    let v = DVec3::Y;

    let DVec2 { x: w, y: h } = key_size;
    let DVec2 { x: cw, y: ch } = corner_size;

    VItem::from_vpoints(
        PathBuilder::new()
            .move_to(DVec3::ZERO)
            .line_to(u * w)
            .line_to(w * u + (ch - h) * v)
            .quad_to(w * u - h * v, (w - cw) * u - h * v)
            .line_to(cw * u - h * v)
            .quad_to(-h * v, (ch - h) * v)
            .close_path()
            .vpoints()
            .to_vec(),
    )
}

fn octave_range(key_range: &Range<u8>) -> Range<u8> {
    let &Range {
        start: k_start,
        end: k_end,
    } = key_range;

    // Index of the first complete octave in the key range
    let o_start = if k_start % 12 == 0 {
        k_start / 12
    } else {
        (k_start / 12) + 1
    };
    // Index of the last complete octave in the key range (not inclusive)
    let o_end = k_end / 12;

    o_start..o_end
}

impl Extract for PianoKeyboard {
    type Target = CoreItem;

    fn extract_into(&self, buf: &mut Vec<Self::Target>) {
        self.keys().extract_into(buf);
    }
}
