use std::{
    array,
    cell::{Ref, RefCell},
    collections::HashMap,
    ops::{Deref, Range},
    slice,
};

use derive_more::{Deref, DerefMut, From, Into};
use itertools::izip;
use music_utils::{
    BLACK_IDX_TO_PREV_WHITE_IDX, BLACK_TONES, KEY_IDX_OF_COLOR, WHITE_TONES,
    black_idx_to_prev_white_idx, is_black_key, is_black_key_otone, key_idx_of_color, octave_range,
};
use ranim::{
    anims::morph::MorphAnim,
    color::{AlphaColor, Srgb, palettes::manim},
    core::{
        Extract, components::width::Width, core_item::CoreItem, num::Integer as _,
        timeline::Timeline,
    },
    glam::{DVec2, DVec3, dvec2},
    items::vitem::{
        VItem,
        geometry::{Rectangle, anchor::Origin},
    },
    prelude::*,
    utils::{bezier::PathBuilder, rate_functions::linear},
};
use ranim_macros::Interpolatable;

/// Size details of piano keyboard keys.
/// All sizes are relative, using white key width as a unit.
#[derive(Clone, Debug, PartialEq, Interpolatable)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PianoKeyboardSize {
    /// Width and height of white keys.
    pub white_height: f64,
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
    pub note_h_scale: [f64; 2],
}

impl PianoKeyboardSize {
    /// Returns the left and right bounds of the whole keyboard in a certain key range.
    /// The top-left corner of the middle C key is used as the origin，
    /// and the width of white key is used as the unit of key size.
    ///
    /// If `clip_black` is `true`, the left- and right-most black keys will be clipped to their neighboring white keys'
    /// left and right edges.
    /// Otherwise, the part the black keys extrude the white keys will be counted.
    pub fn width_range(&self, key_range: &Range<i8>, clip_black: bool) -> Range<f64> {
        let &Range {
            start: tone_start,
            end: tone_end,
        } = key_range;
        let Range {
            start: o_start,
            end: o_end,
        } = octave_range(key_range);
        let otone_start = tone_start.mod_floor(&12) as u8;
        let otone_end = tone_end.mod_floor(&12) as u8;

        let left = if is_black_key_otone(otone_start) {
            let black_idx = key_idx_of_color(otone_start);
            let white_idx = black_idx_to_prev_white_idx(black_idx) + 1;
            if clip_black {
                white_idx as f64
            } else {
                let disp = (1. - self.black_offset[black_idx as usize]) * self.black_size.x / 2.;
                white_idx as f64 - disp
            }
        } else {
            key_idx_of_color(otone_start) as f64
        } + (o_start as f64 - 1.) * 7.;

        let right = if otone_end == 0 {
            0.0
        } else {
            let otone_end = otone_end - 1;
            if is_black_key_otone(otone_end) {
                let black_idx = key_idx_of_color(otone_end);
                let white_idx = black_idx_to_prev_white_idx(black_idx);
                if clip_black {
                    let disp =
                        (1. + self.black_offset[black_idx as usize]) * self.black_size.x / 2.;
                    white_idx as f64 + disp
                } else {
                    white_idx as f64
                }
            } else {
                key_idx_of_color(otone_end) as f64 + 1.
            }
        } + o_end as f64 * 7.;

        left..right
    }

    /// Calculate the width where the left and right sides of a white key overlap with black keys.
    ///
    /// White keys may have top-left or top-right corner cut off by black keys.
    /// The cutoff need to be explicitly drawn instead of using layer overlap to cover
    /// because user may adjust opacity of keys.
    /// If the opacity value is not 1.0, the overlapping parts will become visible.
    pub fn white_key_overlap_widths(&self) -> [[f64; 2]; 7] {
        let mut white_key_cutoff = [[0.; 2]; 7];
        for (black_idx, white_idx) in BLACK_IDX_TO_PREV_WHITE_IDX.iter().copied().enumerate() {
            let offset = self.black_offset[black_idx as usize];
            white_key_cutoff[white_idx as usize][1] = (1. - offset) / 2. * self.black_size.x;
            white_key_cutoff[white_idx as usize + 1][0] = (1. + offset) / 2. * self.black_size.x;
        }
        white_key_cutoff
    }
}

/// Color details of piano keyboard keys.
#[derive(Clone, Debug, PartialEq, Interpolatable)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PianoKeyboardColor {
    /// Fill color of black and white keys.
    pub key_color: [AlphaColor<Srgb>; 2],
    /// Stroke color of keys.
    pub stroke_color: AlphaColor<Srgb>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PianoKeyboardConfig {
    /// Size details of keys.
    pub size: PianoKeyboardSize,
    /// Color details of keys.
    pub color: PianoKeyboardColor,
    /// Range of keys to display.
    pub key_range: Range<i8>,
    /// Stroke width of keys.
    #[serde(skip)]
    pub stroke_width: Width,
}

impl PianoKeyboardConfig {
    pub fn new(key_range: &Range<i8>) -> Self {
        Self {
            key_range: key_range.clone(),
            ..Default::default()
        }
    }

    pub fn width_range(&self, clip_black: bool) -> Range<f64> {
        self.size.width_range(&self.key_range, clip_black)
    }
}

impl Default for PianoKeyboardConfig {
    fn default() -> Self {
        Self {
            size: Default::default(),
            key_range: -39..49, // standard piano keyboard, 88 key
            color: Default::default(),
            stroke_width: Width(0.005),
        }
    }
}

/// A piano keyboard item used for music visualization.
#[derive(Clone, Debug)]
pub struct PianoKeyboard {
    /// Top-left corner of middle C key
    origin: DVec3,
    /// Width of white key, which is used as the unit of key size.
    size_unit: f64,
    /// Other config items irrelevant to absolute size.
    config: PianoKeyboardConfig,
    /// Keys marked with special colors.
    highlighted_keys: HashMap<i8, AlphaColor<Srgb>>,
    /// Cached keys, generated on-demand.
    keys: RefCell<Option<Vec<VItem>>>,
}

impl Deref for PianoKeyboard {
    type Target = PianoKeyboardConfig;

    fn deref(&self) -> &Self::Target {
        &self.config
    }
}

impl Locate<PianoKeyboard> for Origin {
    fn locate(&self, target: &PianoKeyboard) -> DVec3 {
        target.origin
    }
}

#[derive(Clone, Debug, Copy, Default, Deref, DerefMut, From, Into)]
pub struct Tone(pub i8);

impl Locate<PianoKeyboard> for Tone {
    fn locate(&self, target: &PianoKeyboard) -> DVec3 {
        const WHITE_WIDTH_DISP: [u8; 12] = [0, 1, 1, 2, 2, 3, 4, 4, 5, 5, 6, 6];

        let (octave, otone) = self.0.div_mod_floor(&12);
        let otone = otone as u8;

        let unit = target.size_unit;
        let black_width = unit * target.size.black_size.x;
        let white_width = unit;
        let black_offset = target.size.black_offset;

        let mut disp = WHITE_WIDTH_DISP[otone as usize] as f64 * white_width;

        if is_black_key_otone(otone) {
            // is a black key
            let black_idx = key_idx_of_color(otone);
            disp += black_width * (black_offset[black_idx as usize] - 1.) / 2.
        }

        target.origin + (disp + octave as f64 * white_width * 7.) * DVec3::X
    }
}

impl Default for PianoKeyboardSize {
    fn default() -> Self {
        Self {
            white_height: 2. / 0.35,
            black_size: dvec2(0.225 / 0.35, 1.1 / 0.35),
            corner_size: dvec2(0.08 / 0.35, 0.08 / 0.35),
            black_offset: [-0.2, 0.2, -0.2, 0., 0.2],
            note_h_scale: [0.8, 1.],
        }
    }
}

impl Default for PianoKeyboardColor {
    fn default() -> Self {
        Self {
            key_color: [AlphaColor::WHITE, AlphaColor::BLACK],
            stroke_color: manim::GREY_C,
        }
    }
}

impl Default for PianoKeyboard {
    fn default() -> Self {
        Self::from_config(PianoKeyboardConfig::default())
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
    pub fn new(config: PianoKeyboardConfig, origin: DVec3, size_unit: f64) -> Self {
        Self {
            config,
            origin,
            size_unit,
            highlighted_keys: Default::default(),
            keys: Default::default(),
        }
    }

    pub fn from_key_range(key_range: &Range<i8>) -> Self {
        Self::from_config(PianoKeyboardConfig::new(key_range))
    }

    pub fn from_config(config: PianoKeyboardConfig) -> Self {
        Self::new(config, DVec3::ZERO, 0.35)
    }

    pub fn size(&self) -> &PianoKeyboardSize {
        &self.size
    }

    pub fn set_size(&mut self, f: impl FnOnce(&mut PianoKeyboardSize)) -> &mut Self {
        let orig_size = self.size.clone();
        f(&mut self.config.size);
        if self.size != orig_size {
            self.keys.borrow_mut().take();
        }
        self
    }

    pub fn set_size_unit(&mut self, unit: f64) -> &mut Self {
        let orig_unit = self.size_unit;
        self.size_unit = unit;
        if self.size_unit != orig_unit {
            self.keys.borrow_mut().take();
        }
        self
    }

    pub fn set_key_range(&mut self, key_range: Range<i8>) -> &mut Self {
        let orig_range = self.key_range.clone();
        self.config.key_range = key_range.clone();
        if self.key_range != orig_range {
            self.keys.borrow_mut().take();
        }
        self
    }

    pub fn highlighted_keys(&self) -> &HashMap<i8, AlphaColor<Srgb>> {
        &self.highlighted_keys
    }

    pub fn highlight_keys(
        &mut self,
        f: impl FnOnce(&mut HashMap<i8, AlphaColor<Srgb>>),
    ) -> &mut Self {
        let orig_keys = self.highlighted_keys.clone();
        f(&mut self.highlighted_keys);
        if self.highlighted_keys != orig_keys {
            self.set_key_colors();
        }
        self
    }

    pub fn keys(&self) -> Ref<'_, Vec<VItem>> {
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
        tone: i8,
        duration: f64,
        scroll_speed: f64,
        scroll_height: f64,
    ) {
        let unit = self.size_unit;
        let (key_width, h_scale) = if is_black_key(tone) {
            (self.size.black_size.x * unit, self.size.note_h_scale[1])
        } else {
            (unit, self.size.note_h_scale[0])
        };
        let origin = Tone(tone).locate(self) + ((1. - h_scale) * key_width * 0.5) * DVec3::X;
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
            size_unit: unit,
            config:
                PianoKeyboardConfig {
                    size:
                        PianoKeyboardSize {
                            white_height,
                            black_size,
                            corner_size,
                            black_offset,
                            ..
                        },
                    color:
                        PianoKeyboardColor {
                            key_color: [white_color, black_color],
                            stroke_color,
                        },
                    key_range:
                        Range {
                            start: tone_start,
                            end: tone_end,
                        },
                    stroke_width,
                    ..
                },
            ..
        } = self;
        let Range {
            start: o_start,
            end: o_end,
        } = octave_range(&self.key_range);

        // convert relative sizes to absolute sizes
        let black_size = black_size * unit;
        let white_size = dvec2(unit, white_height * unit);
        let corner_size = corner_size * unit;

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
        let white_key_cutoff = self
            .size
            .white_key_overlap_widths()
            .map(|v| v.map(|v| v * unit));

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
        for (step, tone, offset) in izip!(BLACK_IDX_TO_PREV_WHITE_IDX, BLACK_TONES, black_offset) {
            i_octave[tone as usize] = i_black_key.clone().with(|item| {
                item.shift(
                    u * (white_size.x * (step as f64 + 1.) + black_size.x * (offset - 1.) / 2.),
                );
            });
        }

        let mut keys = Vec::with_capacity(self.key_range.len());
        if o_end < o_start {
            // Start and end are inside the same octave.

            if tone_end > tone_start {
                let otone_start = (tone_start - o_end * 12) as u8;
                let otone_end = (tone_end - o_end * 12) as u8;

                // First white key has no cutoff on the left side.
                let idx = otone_start as usize;
                let first_key = if is_black_key_otone(otone_start) {
                    i_octave[idx].clone()
                } else {
                    let idx = KEY_IDX_OF_COLOR[idx] as usize;
                    create_white_key(idx, [0., white_key_cutoff[idx][1]])
                };
                keys.push(first_key.with(|item| {
                    item.shift((o_end as f64 - 1.) * 7. * white_size.x * u + origin)
                        .discard()
                }));

                if otone_end > 0 {
                    // Other keys in the middle.
                    for i in (otone_start + 1)..(otone_end - 1) {
                        keys.push(i_octave[i as usize].clone().with(|item| {
                            item.shift(o_end as f64 * 7. * white_size.x * u + origin)
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
                        item.shift(o_end as f64 * 7. * white_size.x * u + origin)
                            .discard()
                    }));
                }
            }
        } else {
            // Crosses multiple octaves.

            // First incomplete octave
            {
                // First white key has no cutoff on the left side.
                let otone_start = (tone_start - (o_start - 1) * 12) as u8;
                let idx = otone_start as usize;
                let first_key = if is_black_key_otone(otone_start) {
                    i_octave[idx].clone()
                } else {
                    let idx = KEY_IDX_OF_COLOR[idx] as usize;
                    create_white_key(idx, [0., white_key_cutoff[idx][1]])
                };
                keys.push(first_key.with(|item| {
                    item.shift((o_start as f64 - 1.) * 7. * white_size.x * u + origin)
                        .discard()
                }));

                // Other keys in the first octave.
                for i in (otone_start + 1)..12 {
                    keys.push(i_octave[i as usize].clone().with(|item| {
                        item.shift((o_start as f64 - 1.) * 7. * white_size.x * u + origin)
                            .discard()
                    }));
                }
            }

            // Complete octaves in the middle.
            for o in o_start..o_end {
                keys.extend(i_octave.clone().with(|item| {
                    item.shift(o as f64 * 7. * white_size.x * u + origin)
                        .discard()
                }));
            }

            // last incomplete octave
            {
                // Other keys in the last octave.
                let otone_end = (tone_end - o_end * 12) as u8;
                // If `otone_end == 0` then the last octave must be empty
                // so no key needs to be created.
                // In this case `otone_end - 1` will cause overflow.
                if otone_end > 0 {
                    for i in 0..(otone_end - 1) {
                        keys.push(i_octave[i as usize].clone().with(|item| {
                            item.shift(o_end as f64 * 7. * white_size.x * u + origin)
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
                        item.shift(o_end as f64 * 7. * white_size.x * u + origin)
                            .discard()
                    }));
                }
            }
        }

        keys
    }

    fn set_key_colors(&self) {
        let &Self {
            config:
                PianoKeyboardConfig {
                    color:
                        PianoKeyboardColor {
                            key_color: [white_color, black_color],
                            ..
                        },
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
                        // [TODO] allow custom color transformation for black keys?
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
        self.config.color.stroke_color = self.color.stroke_color.with_alpha(opacity);
        self.transform_items(|item| item.set_stroke_opacity(opacity).discard());
        self
    }

    fn set_stroke_color(&mut self, color: AlphaColor<Srgb>) -> &mut Self {
        self.config.color.stroke_color = color;
        self.transform_items(|item| item.set_stroke_color(color).discard());
        self
    }
}

impl StrokeWidth for PianoKeyboard {
    fn stroke_width(&self) -> f32 {
        self.stroke_width.0
    }

    fn apply_stroke_func(&mut self, f: impl for<'a> Fn(&'a mut [Width])) -> &mut Self {
        f(slice::from_mut(&mut self.config.stroke_width));
        self
    }
}

impl ScaleTransform for PianoKeyboard {
    fn scale(&mut self, scale: DVec3) -> &mut Self {
        self.origin.scale(scale);
        self.size_unit *= scale.x;

        let ratio = scale.y / scale.x;
        self.config.size.white_height *= ratio;
        self.config.size.black_size.y *= ratio;
        self.config.size.corner_size.y *= ratio;

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

/// Generate path for a white piano key with top-left or top-right corner cut off
fn piano_key_white(
    key_size: DVec2,
    corner_size: DVec2,
    cutoff_widths: [f64; 2],
    cutoff_height: f64,
) -> Vec<DVec3> {
    let u = DVec3::X;
    let v = DVec3::Y;

    let [bw1, bw2] = cutoff_widths;
    let bh = cutoff_height;
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
