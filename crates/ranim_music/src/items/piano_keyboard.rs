use std::{
    array,
    cell::{Ref, RefCell},
    collections::HashMap,
    ops::Range,
    slice,
};

use itertools::izip;
use ranim::{
    anims::morph::MorphAnim,
    color::{AlphaColor, Srgb, palettes::manim},
    core::{Extract, components::width::Width, core_item::CoreItem, timeline::Timeline},
    glam::{DVec2, DVec3, Vec3Swizzles as _, dvec2},
    items::vitem::{
        VItem,
        geometry::{Rectangle, anchor::Origin},
    },
    prelude::*,
    utils::{bezier::PathBuilder, rate_functions::linear},
};
use ranim_macros::Interpolatable;

const BLACK_KEY_MASK: u16 = 0b0101010_01010;
const KEY_IDX_OF_COLOR: [u8; 12] = [0, 0, 1, 1, 2, 3, 2, 4, 3, 5, 4, 6];
const WHITE_TONES: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const BLACK_TONES: [u8; 5] = [1, 3, 6, 8, 10];
/// White key index on the left side of each black key.
const NEIGHBORING_WHITE_KEYS: [u8; 5] = [0, 1, 3, 4, 5];

#[inline(always)]
fn is_black_key_otone(otone: u8) -> bool {
    BLACK_KEY_MASK & (1 << otone) != 0
}

#[inline(always)]
fn is_black_key(key: u8) -> bool {
    is_black_key_otone(key % 12)
}

/// Size details of piano keyboard keys.
#[derive(Clone, Debug, PartialEq, Interpolatable)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PianoKeyboardSize {
    /// Width and height of white keys.
    pub white_size: DVec2,
    /// Width and height of black keys.
    pub black_size: DVec2,
    /// Width and height of round corners at the bottom of keys.
    pub corner_size: DVec2,
    /// Displacement of black keys from the gap between white keys.
    /// Measured in multiples of half black key width.
    /// The values should typically be in the range `-1.0..=1.0`.
    /// `0.0` means the black key is perfectly centered between two white keys.
    pub black_offset: [f64; 5],
    /// Scale of note widths.
    pub note_h_scale: (f64, f64),
}

/// Color details of piano keyboard keys.
#[derive(Clone, Debug, PartialEq, Interpolatable)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PianoKeyboardColor {
    /// Fill color of black and white keys.
    key_colors: (AlphaColor<Srgb>, AlphaColor<Srgb>),
    /// Stroke color of keys.
    stroke_color: AlphaColor<Srgb>,
}

/// A piano keyboard item used for music visualization.
#[derive(Clone, Debug)]
pub struct PianoKeyboard {
    /// Top-left corner of middle C key
    origin: DVec3,
    /// Size details of keys.
    size: PianoKeyboardSize,
    /// Color details of keys.
    color: PianoKeyboardColor,
    /// Range of keys to display.
    key_range: Range<u8>,
    /// Keys marked with special colors.
    highlighted_keys: HashMap<u8, AlphaColor<Srgb>>,
    /// Stroke width of keys.
    stroke_width: Width,
    /// Cached keys, generated on-demand.
    keys: RefCell<Option<Vec<VItem>>>,
}

impl Locate<PianoKeyboard> for Origin {
    fn locate(&self, target: &PianoKeyboard) -> DVec3 {
        target.origin
    }
}

impl Locate<PianoKeyboard> for u8 {
    fn locate(&self, target: &PianoKeyboard) -> DVec3 {
        const WHITE_WIDTH_DISP: [u8; 12] = [0, 1, 1, 2, 2, 3, 4, 4, 5, 5, 6, 6];

        let (octave, otone) = (self / 12, self % 12);

        let black_width = target.size.black_size.x;
        let white_width = target.size.white_size.x;
        let black_offset = target.size.black_offset;

        let mut disp = WHITE_WIDTH_DISP[otone as usize] as f64 * white_width;

        if is_black_key_otone(otone) {
            // is a black key
            let black_idx = KEY_IDX_OF_COLOR[otone as usize];
            disp += black_width * (black_offset[black_idx as usize] - 1.) / 2.
        }

        target.origin + (disp + (octave as f64 - 5.) * white_width * 7.) * DVec3::X
    }
}

impl Default for PianoKeyboardSize {
    fn default() -> Self {
        Self {
            white_size: dvec2(0.35, 2.),
            black_size: dvec2(0.225, 1.1),
            corner_size: dvec2(0.08, 0.08),
            black_offset: [-0.2, 0.2, -0.2, 0.0, 0.2],
            note_h_scale: (0.8, 1.),
        }
    }
}

impl Default for PianoKeyboardColor {
    fn default() -> Self {
        Self {
            key_colors: (AlphaColor::WHITE, AlphaColor::BLACK),
            stroke_color: manim::GREY_C,
        }
    }
}

impl Default for PianoKeyboard {
    fn default() -> Self {
        Self {
            origin: DVec3::ZERO,
            size: Default::default(),
            key_range: 21..109, // standard piano keyboard, 88 key
            highlighted_keys: Default::default(),
            color: Default::default(),
            keys: Default::default(),
            stroke_width: Width(0.005),
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
        self.transform_items(|item| item.shift(shift).discard());
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

    pub fn anim_note(
        &self,
        tl: &mut Timeline,
        note_setup: impl Fn(&mut Rectangle),
        tone: u8,
        duration: f64,
        scroll_speed: f64,
        scroll_height: f64,
    ) {
        let (key_width, h_scale) = if is_black_key(tone) {
            (self.size.black_size.x, self.size.note_h_scale.1)
        } else {
            (self.size.white_size.x, self.size.note_h_scale.0)
        };
        let origin = tone.locate(self) + ((1. - h_scale) * key_width * 0.5) * DVec3::X;
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

        let &Self {
            origin,
            size:
                PianoKeyboardSize {
                    white_size,
                    black_size,
                    corner_size,
                    black_offset,
                    ..
                },
            color:
                PianoKeyboardColor {
                    key_colors: (white_color, black_color),
                    stroke_color,
                },
            key_range:
                Range {
                    start: tone_start,
                    end: tone_end,
                },
            stroke_width,
            ..
        } = self;
        let Range {
            start: o_start,
            end: o_end,
        } = octave_range(&self.key_range);

        // Generate piano keys in an octave.
        let mut i_octave: [VItem; 12] = array::from_fn(|_| VItem::empty());
        // black keys are all the same with rectangular shape
        let i_black_key = VItem::from_vpoints(piano_key(black_size, corner_size)).with(|item| {
            item.set_fill_color(black_color)
                .set_stroke_color(stroke_color)
                .set_stroke_width(stroke_width.0);
        });
        // White keys may have top-left or top-right corner cut off by black keys.
        // The cutoff need to be explicitly drawn instead of using layer overlap to cover
        // because user may adjust opacity of keys.
        // If the opacity value is not 1.0, the overlapping parts will become visible.
        let white_key_cutoff = {
            let mut white_key_cutoff = [[0.; 2]; 7];
            for (black_idx, white_idx) in NEIGHBORING_WHITE_KEYS.iter().copied().enumerate() {
                let offset = black_offset[black_idx as usize];
                white_key_cutoff[white_idx as usize][1] = (1. - offset) / 2. * black_size.x;
                white_key_cutoff[white_idx as usize + 1][0] = (1. + offset) / 2. * black_size.x;
            }
            white_key_cutoff
        };
        let create_white_key = |step: usize, cutoff: [f64; 2]| {
            VItem::from_vpoints(piano_key_white(
                white_size,
                corner_size,
                cutoff,
                black_size.y,
            ))
            .with(|item| {
                item.set_fill_color(white_color)
                    .set_stroke_color(stroke_color)
                    .set_stroke_width(stroke_width.0)
                    .shift(white_size.x * step as f64 * u)
                    .discard()
            })
        };
        let white_keys = white_key_cutoff
            .iter()
            .copied()
            .enumerate()
            .map(|(step, black_widths)| create_white_key(step, black_widths));
        for (tone, item) in izip!(WHITE_TONES, white_keys) {
            i_octave[tone as usize] = item;
        }
        for (step, tone, offset) in izip!(NEIGHBORING_WHITE_KEYS, BLACK_TONES, black_offset) {
            i_octave[tone as usize] = i_black_key.clone().with(|item| {
                item.shift(
                    u * (white_size.x * (step as f64 + 1.) + black_size.x * (offset - 1.) / 2.),
                );
            });
        }

        let mut keys = Vec::with_capacity(self.key_range.len());
        if o_end < o_start {
            if tone_end > tone_start {
                // Start and end are inside the same octave.
                let otone_start = tone_start - o_end * 12;
                let otone_end = tone_end - o_end * 12;

                // First white key has no cutoff on the left side.
                let idx = otone_start as usize;
                let first_key = if is_black_key_otone(otone_start) {
                    i_octave[idx].clone()
                } else {
                    let idx = KEY_IDX_OF_COLOR[idx] as usize;
                    create_white_key(idx, [0., white_key_cutoff[idx][1]])
                };
                keys.push(first_key.with(|item| {
                    item.shift((o_end as f64 - 6.) * 7. * white_size.x * u + origin)
                        .discard()
                }));

                if otone_end > 0 {
                    // Other keys in the middle.
                    for i in (otone_start + 1)..(otone_end - 1) {
                        keys.push(i_octave[i as usize].clone().with(|item| {
                            item.shift((o_end as f64 - 5.) * 7. * white_size.x * u + origin)
                                .discard()
                        }));
                    }

                    // Last white key has no cutoff on the right side.
                    let idx = otone_end as usize - 1;
                    let last_key = if is_black_key_otone(otone_end - 1) {
                        i_octave[idx].clone()
                    } else {
                        let idx = KEY_IDX_OF_COLOR[idx] as usize;
                        create_white_key(idx, [white_key_cutoff[idx][0], 0.])
                    };
                    keys.push(last_key.with(|item| {
                        item.shift((o_end as f64 - 5.) * 7. * white_size.x * u + origin)
                            .discard()
                    }));
                }
            }
        } else {
            // Crosses multiple octaves.

            // First white key has no cutoff on the left side.
            let otone_start = tone_start - (o_start - 1) * 12;
            let idx = otone_start as usize;
            let first_key = if is_black_key_otone(otone_start) {
                i_octave[idx].clone()
            } else {
                let idx = KEY_IDX_OF_COLOR[idx] as usize;
                create_white_key(idx, [0., white_key_cutoff[idx][1]])
            };
            keys.push(first_key.with(|item| {
                item.shift((o_start as f64 - 6.) * 7. * white_size.x * u + origin)
                    .discard()
            }));

            // Other keys in the first octave.
            for i in (otone_start + 1)..12 {
                keys.push(i_octave[i as usize].clone().with(|item| {
                    item.shift((o_start as f64 - 6.) * 7. * white_size.x * u + origin)
                        .discard()
                }));
            }

            // Complete octaves in the middle.
            for o in o_start..o_end {
                keys.extend(i_octave.clone().with(|item| {
                    item.shift((o as f64 - 5.) * 7. * white_size.x * u + origin)
                        .discard()
                }));
            }

            // Other keys in the last octave.
            let otone_end = tone_end - o_end * 12;
            // If `otone_end == 0` then the last octave must be empty
            // so no key needs to be created.
            // In this case `otone_end - 1` will cause overflow.
            if otone_end > 0 {
                for i in 0..(otone_end - 1) {
                    keys.push(i_octave[i as usize].clone().with(|item| {
                        item.shift((o_end as f64 - 5.) * 7. * white_size.x * u + origin)
                            .discard()
                    }));
                }

                // Last white key has no cutoff on the right side.
                let idx = otone_end as usize - 1;
                let last_key = if is_black_key_otone(otone_end - 1) {
                    i_octave[idx].clone()
                } else {
                    let idx = KEY_IDX_OF_COLOR[idx] as usize;
                    create_white_key(idx, [white_key_cutoff[idx][0], 0.])
                };
                keys.push(last_key.with(|item| {
                    item.shift((o_end as f64 - 5.) * 7. * white_size.x * u + origin)
                        .discard()
                }));
            }
        }

        keys
    }

    fn set_key_colors(&self) {
        let &Self {
            color:
                PianoKeyboardColor {
                    key_colors: (white_color, black_color),
                    ..
                },
            ..
        } = self;
        if let Some(i_keys) = self.keys.borrow_mut().as_mut() {
            self.key_range
                .clone()
                .zip(i_keys.iter_mut())
                .for_each(|(tone, i_key)| {
                    i_key
                        .set_fill_color(if is_black_key(tone) {
                            black_color
                        } else {
                            white_color
                        })
                        .discard()
                });
            self.highlighted_keys.iter().for_each(|(&tone, &color)| {
                let idx = tone - self.key_range.start;
                if let Some(key) = i_keys.get_mut(idx as usize) {
                    key.set_fill_color(if is_black_key(tone) {
                        color
                            .map_lightness(|x| x - 0.2)
                            .with_alpha(black_color.components[3]) // match alpha with key color
                    } else {
                        color.with_alpha(white_color.components[3]) // match alpha with key color
                    });
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

impl StrokeColor for PianoKeyboard {
    fn stroke_color(&self) -> AlphaColor<Srgb> {
        self.color.stroke_color
    }

    fn set_stroke_opacity(&mut self, opacity: f32) -> &mut Self {
        self.color.stroke_color = self.color.stroke_color.with_alpha(opacity);
        self.transform_items(|item| item.set_stroke_opacity(opacity).discard());
        self
    }

    fn set_stroke_color(&mut self, color: AlphaColor<Srgb>) -> &mut Self {
        self.color.stroke_color = color;
        self.transform_items(|item| item.set_stroke_color(color).discard());
        self
    }
}

impl StrokeWidth for PianoKeyboard {
    fn stroke_width(&self) -> f32 {
        self.stroke_width.0
    }

    fn apply_stroke_func(&mut self, f: impl for<'a> Fn(&'a mut [Width])) -> &mut Self {
        f(slice::from_mut(&mut self.stroke_width));
        self
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

/// Generate path for a complete piano key
fn piano_key(key_size: DVec2, corner_size: DVec2) -> Vec<DVec3> {
    let u = DVec3::X;
    let v = DVec3::Y;

    let DVec2 { x: w, y: h } = key_size;
    let DVec2 { x: cw, y: ch } = corner_size;

    PathBuilder::new()
        .move_to(DVec3::ZERO)
        .line_to(u * w)
        .line_to(w * u + (ch - h) * v)
        .quad_to(w * u - h * v, (w - cw) * u - h * v)
        .line_to(cw * u - h * v)
        .quad_to(-h * v, (ch - h) * v)
        .close_path()
        .vpoints()
        .to_vec()
}

#[allow(unused)]
fn piano_key_white(
    key_size: DVec2,
    corner_size: DVec2,
    black_key_widths: [f64; 2],
    black_key_height: f64,
) -> Vec<DVec3> {
    let u = DVec3::X;
    let v = DVec3::Y;

    let [bw1, bw2] = black_key_widths;
    let bh = black_key_height;
    let DVec2 { x: w, y: h } = key_size;
    let DVec2 { x: cw, y: ch } = corner_size;

    let mut pb = PathBuilder::new();

    if bw1 > 0. {
        pb.move_to(bw1 * u);
    } else {
        pb.move_to(DVec3::ZERO);
    }

    if bw2 > 0. {
        pb.line_to((w - bw2) * u)
            .line_to((w - bw2) * u - (bh - ch) * v)
            .quad_to((w - bw2) * u - bh * v, (w - bw2 + cw) * u - bh * v)
            .line_to(w * u - bh * v);
    } else {
        pb.line_to(w * u);
    }

    pb.line_to(w * u - (h - ch) * v)
        .quad_to(w * u - h * v, (w - cw) * u - h * v)
        .line_to(cw * u - h * v)
        .quad_to(-h * v, (-h + ch) * v);

    if bw1 > 0. {
        pb.line_to(-bh * v)
            .line_to((bw1 - cw) * u - bh * v)
            .quad_to(bw1 * u - bh * v, bw1 * u - (bh - ch) * v);
    }

    pb.close_path().vpoints().to_vec()
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

impl From<PianoKeyboard> for Vec<VItem> {
    fn from(piano_keyboard: PianoKeyboard) -> Self {
        piano_keyboard.keys().clone()
    }
}

impl Extract for PianoKeyboard {
    type Target = CoreItem;

    fn extract_into(&self, buf: &mut Vec<Self::Target>) {
        self.keys().extract_into(buf);
    }
}
