#![allow(non_camel_case_types)]

use crate::utils::{nano_to_time_string, to_egui_color, to_egui_vec2, to_ranim_dvec2};
use eframe::{
    egui::{self, Response, Sense, Ui},
    emath::OrderedFloat,
    epaint,
};
use music_utils::{
    KeyInfo, black_idx_to_prev_white_idx, black_tone, is_black_key, is_black_key_otone,
    key_idx_of_color, key_info, octave_range, white_idx_to_next_black_idx, white_tone,
};
use ranim::{
    SceneConfig,
    cmd::preview::Resolution,
    color::{AlphaColor, try_color},
    glam::DVec2,
};
use ranim_midi_visualizer_lib::{
    ColorBy, MidiVisualizerConfig, ProgressBarConfig, StatusBarConfig, cyc_index::IndexCyc,
};
use std::{collections::HashMap, f32::consts::PI as PI_f32, ops::Range};
use structured_midi::{MidiMusic, MultiTrackMidiNote};

type f64o = OrderedFloat<f64>;

fn points_on_circ(
    center: egui::Pos2,
    radius: egui::Vec2,
    start_angle: f32,
    span_angle: f32,
    segments: usize,
) -> impl ExactSizeIterator<Item = egui::Pos2> {
    (0..segments + 1).map(move |v| {
        let theta = v as f32 / segments as f32 * span_angle + start_angle;
        center + egui::vec2(theta.cos(), theta.sin()) * radius
    })
}

#[derive(Clone, Debug)]
struct PianoKeyShape {
    origin: egui::Pos2,
    size: egui::Vec2,
    corner_size: egui::Vec2,
    fill_color: egui::Color32,
    stroke: epaint::PathStroke,
    cutoff: [Option<egui::Vec2>; 2],
}

impl From<PianoKeyShape> for epaint::PathShape {
    fn from(value: PianoKeyShape) -> Self {
        // [FIXME] `epaint::PathShape` now fills the convex hull of vertices instead of the actual polygon.
        // Maybe use `epaint::Mesh` instead

        const ARC_SEGMENTS: usize = 8;
        #[allow(non_upper_case_globals)]
        const HALF_PI_f32: f32 = PI_f32 / 2.;

        let PianoKeyShape {
            origin,
            size,
            corner_size,
            fill_color,
            stroke,
            cutoff,
        } = value;
        let mut points = Vec::new();

        // right side cutoff
        if let Some(cutoff_right) = cutoff[1] {
            points.push(egui::pos2(origin.x + size.x - cutoff_right.x, origin.y));
            points.extend(points_on_circ(
                egui::pos2(
                    origin.x + size.x - cutoff_right.x + corner_size.x,
                    origin.y + cutoff_right.y - corner_size.y,
                ),
                corner_size,
                PI_f32,
                -HALF_PI_f32,
                ARC_SEGMENTS,
            ));
            points.push(egui::pos2(origin.x + size.x, origin.y + cutoff_right.y));
        } else {
            points.push(egui::pos2(origin.x + size.x, origin.y));
        }

        // bottom
        points.extend(points_on_circ(
            origin + size - corner_size,
            corner_size,
            0.,
            HALF_PI_f32,
            ARC_SEGMENTS,
        ));
        points.extend(points_on_circ(
            origin + egui::vec2(corner_size.x, size.y - corner_size.y),
            corner_size,
            HALF_PI_f32,
            HALF_PI_f32,
            ARC_SEGMENTS,
        ));

        // left side cutoff
        if let Some(cutoff_left) = cutoff[0] {
            points.push(egui::pos2(origin.x, origin.y + cutoff_left.y));
            points.extend(points_on_circ(
                egui::pos2(
                    origin.x + cutoff_left.x - corner_size.x,
                    origin.y + cutoff_left.y - corner_size.y,
                ),
                corner_size,
                HALF_PI_f32,
                -HALF_PI_f32,
                ARC_SEGMENTS,
            ));
            points.push(egui::pos2(origin.x + cutoff_left.x, origin.y));
        } else {
            points.push(origin);
        }

        epaint::PathShape {
            points,
            closed: true,
            fill: fill_color,
            stroke,
        }
    }
}

impl From<PianoKeyShape> for egui::Shape {
    fn from(value: PianoKeyShape) -> Self {
        epaint::PathShape::from(value).into()
    }
}

#[derive(Default, Clone, Debug)]
pub struct DataCache {
    pub nps: Option<f64>,
    pub nps_max: Option<f64>,
    pub legato_index: Option<f64>,
    pub note_count: Option<usize>,
    pub note_count_total: Option<usize>,
}

/// A preview widget for MIDI visualizer.
#[allow(unused)]
pub struct MidiVisualizerPreview<'a> {
    /// the displaying MIDI music
    music: &'a MidiMusic,
    /// configuration of the MIDI visualizer
    pub visualizer_config: &'a MidiVisualizerConfig,
    /// configuration of the Ranim scene
    pub scene_config: &'a SceneConfig,
    /// output video resolution
    pub resolution: Resolution,
    /// Time window for calculating NPS and legato index
    pub window: u64,
    /// current playing time in nanoseconds
    pub time: u64,
    /// cached metric data to avoid repetitive calculation
    pub cache: DataCache,
}

impl<'a> MidiVisualizerPreview<'a> {
    pub fn new(
        music: &'a MidiMusic,
        visualizer_config: &'a MidiVisualizerConfig,
        scene_config: &'a SceneConfig,
        resolution: Resolution,
        window: u64,
    ) -> Self {
        Self {
            music,
            visualizer_config,
            scene_config,
            resolution,
            window,
            time: 0,
            cache: Default::default(),
        }
    }
}

impl<'a> egui::Widget for MidiVisualizerPreview<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let total_time = self.music.duration();

        let available_size = ui.available_size();
        let aspect_ratio = self.resolution.ratio();
        let size = {
            let mut size = available_size;
            if size.x / size.y > aspect_ratio {
                size.x = size.y * aspect_ratio;
            } else {
                size.y = size.x / aspect_ratio;
            }
            size
        };

        ui.centered_and_justified(|ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::all());

            let egui_view_width = rect.width();
            #[allow(unused)]
            let egui_view_height = rect.height();
            let egui_view_top_left = rect.left_top();
            let egui_view_bottom_left = rect.left_bottom();

            #[allow(non_upper_case_globals)]
            const ranim_view_height: f64 = 8.;
            // let ranim_view_width = egui_view_width * aspect_ratio;

            // Ranim's origin represented in UI coordinates.
            let origin = rect.center();

            // number of UI coordinate units per Ranim coordinate unit.
            // defining Ranim's unit as 1/8 the view height, which is the default value of Ranim's `CameraFrame`.
            let unit = rect.height() / ranim_view_height as f32;

            // conversion from UI to Ranim coordinates.
            #[allow(unused)]
            let to_ui_coord = |ranim_coord: DVec2| -> egui::Pos2 {
                // y coordinate needs to be flipped because Ranim's y-axis points upwards.
                origin + egui::vec2(ranim_coord.x as f32, -ranim_coord.y as f32) * unit
            };

            // conversion from Ranim to UI coordinates.
            #[allow(unused)]
            let to_ranim_coord = |ui_coord: egui::Pos2| -> DVec2 {
                let mut v = (ui_coord - origin) / unit;
                // flip y coordinate back.
                v.y = -v.y;
                to_ranim_dvec2(v)
            };

            let p = ui.painter();

            // background color
            {
                let fill_color =
                    try_color(&self.scene_config.clear_color).unwrap_or(AlphaColor::TRANSPARENT);
                let fill_color = to_egui_color(fill_color);
                p.rect_filled(rect, 0., fill_color);
            }

            // status bar
            let status_bar_config = &self.visualizer_config.status_bar_config;
            let egui_status_bar_height = status_bar_config.height() as f32 * unit;
            {
                let &StatusBarConfig {
                    em_size: ranim_em_size,
                    padding: ranim_padding,
                    bg_color,
                    fg_color,
                } = status_bar_config;
                let bg_color = to_egui_color(bg_color);
                let fg_color = to_egui_color(fg_color);

                let egui_pad_left = ranim_padding[0].x as f32 * unit;
                let egui_pad_right = ranim_padding[1].x as f32 * unit;
                let egui_pad_bottom = ranim_padding[1].y as f32 * unit;
                let egui_em_size = ranim_em_size as f32 * unit;
                let egui_content_width = egui_view_width - egui_pad_left - egui_pad_right;

                // background rectangle
                let bg_rect = egui::Rect::from_min_size(
                    egui_view_bottom_left - egui::vec2(0., egui_status_bar_height),
                    egui::vec2(egui_view_width, egui_status_bar_height),
                );
                p.rect_filled(bg_rect, 0., bg_color);

                let create_text = |text: &str, column: usize| {
                    const N_COLUMNS: usize = 4;

                    let egui_text_origin = egui_view_bottom_left
                        + egui::vec2(
                            egui_pad_left + egui_content_width * (column as f32 / N_COLUMNS as f32),
                            -egui_pad_bottom,
                        );
                    p.text(
                        egui_text_origin,
                        egui::Align2::LEFT_BOTTOM,
                        text,
                        egui::FontId::monospace(egui_em_size),
                        fg_color,
                    );
                };

                let note_count = self.cache.note_count.unwrap_or_else(|| {
                    self.music
                        .note_count_iter()
                        .take_while(|&(time, _)| time <= self.time)
                        .last()
                        .map(|v| v.1)
                        .unwrap_or(0)
                });
                let note_count_total = self.cache.note_count_total.unwrap_or_else(|| {
                    self.music
                        .note_count_iter()
                        .last()
                        .map(|v| v.1)
                        .unwrap_or(0)
                });
                let nps = self
                    .cache
                    .nps
                    .unwrap_or_else(|| self.music.nps(self.time, self.window));
                let nps_max = self.cache.nps_max.unwrap_or_else(|| {
                    self.music
                        .nps_iter(self.window)
                        .take_while(|&(time, _)| time <= self.time)
                        .map(|(_, nps)| f64o::from(nps))
                        .max()
                        .map(|v| v.into_inner())
                        .unwrap_or(0.0)
                });
                let legato_index = self
                    .cache
                    .legato_index
                    .unwrap_or_else(|| self.music.legato_index(self.time, self.window));

                create_text(
                    format!("TIME {}", nano_to_time_string(self.time)).as_str(),
                    0,
                );
                create_text(
                    format!("NOTE COUNT {note_count} / {note_count_total}",).as_str(),
                    1,
                );
                create_text(format!("NPS (MAX) {nps:.0} ({nps_max:.0})").as_str(), 2);
                create_text(format!("LEGATO {legato_index:.3}").as_str(), 3);
            }

            // progress bar
            {
                let progress = if total_time == 0 {
                    0.
                } else {
                    (self.time as f64 / total_time as f64) as f32
                };
                let progress_bar_config = &self.visualizer_config.progress_bar_config;

                let &ProgressBarConfig {
                    height: ranim_height,
                    fg_color,
                    bg_color,
                } = progress_bar_config;
                let egui_height = ranim_height as f32 * unit;
                let bg_color = to_egui_color(bg_color);
                let fg_color = to_egui_color(fg_color);

                let bg_rect = egui::Rect::from_min_size(
                    egui_view_top_left,
                    egui::vec2(egui_view_width, egui_height),
                );
                let fg_rect = egui::Rect::from_min_size(
                    egui_view_top_left,
                    egui::vec2(egui_view_width * progress, egui_height),
                );

                p.rect_filled(bg_rect, 0., bg_color);
                p.rect_filled(fg_rect, 0., fg_color);
            }

            // Keys and notes
            {
                let key_range = &self.visualizer_config.keyboard_config.key_range;
                let keyboard_size = &self.visualizer_config.keyboard_config.size;
                let keyboard_color = &self.visualizer_config.keyboard_config.color;
                let color_by = self.visualizer_config.color_by;
                let note_colors = self.visualizer_config.colors.as_slice();

                let highlighted_keys: HashMap<_, _> = {
                    let time_range = self.time..self.time;
                    let notes_on = self
                        .music
                        .notes_between_iter(&time_range, key_range)
                        .map(|(_, note)| note);
                    {
                        use ColorBy::*;
                        match color_by {
                            Channel => notes_on
                                .map(|note| {
                                    (note.key, *note_colors.index_cyc(note.loc.channel as usize))
                                })
                                .collect(),
                            Track => notes_on
                                .map(|note| {
                                    (note.key, *note_colors.index_cyc(note.loc.track as usize))
                                })
                                .collect(),
                            KeyColor => notes_on
                                .map(|note| {
                                    (
                                        note.key,
                                        *note_colors.index_cyc(is_black_key(note.key) as usize),
                                    )
                                })
                                .collect(),
                        }
                    }
                };

                let &Range {
                    start: tone_start,
                    end: tone_end,
                } = key_range;
                let Range {
                    start: left,
                    end: right,
                } = keyboard_size.width_range(key_range, false);
                let Range {
                    start: o_start,
                    end: o_end,
                } = octave_range(key_range);

                let egui_key_unit = egui_view_width / (right - left) as f32;
                let egui_keyboard_height = egui_key_unit * keyboard_size.white_height as f32;
                let egui_key_origin = egui_view_bottom_left
                    - egui::vec2(
                        left as f32 * egui_key_unit,
                        egui_status_bar_height + egui_keyboard_height,
                    );

                let egui_white_size = egui::vec2(egui_key_unit, egui_keyboard_height);
                let egui_black_size = egui_key_unit * to_egui_vec2(keyboard_size.black_size);
                let egui_corner_size = egui_key_unit * to_egui_vec2(keyboard_size.corner_size);

                let (white_color, black_color) = keyboard_color.key_color;
                let egui_white_color = to_egui_color(white_color);
                let egui_black_color = to_egui_color(black_color);

                let egui_stroke_color = to_egui_color(keyboard_color.stroke_color);

                let egui_stroke_width =
                    self.visualizer_config.keyboard_config.stroke_width.0 * unit;
                let egui_stroke = epaint::PathStroke::new(egui_stroke_width, egui_stroke_color);

                let egui_overlap_height = keyboard_size.black_size.y as f32 * egui_key_unit;
                let egui_overlaps = keyboard_size.white_key_overlap_widths().map(|v| {
                    v.map(|v| {
                        if v <= 0. {
                            None
                        } else {
                            Some(egui::vec2(v as f32 * egui_key_unit, egui_overlap_height))
                        }
                    })
                });

                let white_key_origin = |octave: i8, white_idx: u8| {
                    egui_key_origin
                        + egui::vec2((white_idx as f32 + octave as f32 * 7.) * egui_key_unit, 0.)
                };

                let black_key_origin = |octave: i8, black_idx: u8| {
                    let white_idx = black_idx_to_prev_white_idx(black_idx);
                    let disp = keyboard_size.black_offset[black_idx as usize];
                    egui_key_origin
                        + egui::vec2(
                            (white_idx as f32 + octave as f32 * 7. + 1.) * egui_key_unit
                                + egui_black_size.x * (disp - 1.) as f32 / 2.,
                            0.,
                        )
                };

                let key_origin_and_color = |key: i8| {
                    let KeyInfo {
                        octave,
                        is_black,
                        idx_of_color,
                    } = key_info(key);
                    (
                        if is_black {
                            black_key_origin(octave, idx_of_color)
                        } else {
                            white_key_origin(octave, idx_of_color)
                        },
                        is_black,
                    )
                };

                // Piano keys
                {
                    // create a white key shape
                    let white_key_shape = |octave: i8, white_idx: u8, cutoff_mask: [bool; 2]| {
                        let origin = white_key_origin(octave, white_idx);
                        let tone = white_tone(octave, white_idx);
                        PianoKeyShape {
                            origin,
                            size: egui_white_size,
                            corner_size: egui_corner_size,
                            fill_color: highlighted_keys
                                .get(&tone)
                                .map(|&v| to_egui_color(v.with_alpha(white_color.components[3])))
                                .unwrap_or_else(|| egui_white_color),
                            stroke: egui_stroke.clone(),
                            cutoff: std::array::from_fn(|i| {
                                if cutoff_mask[i] {
                                    egui_overlaps[white_idx as usize][i]
                                } else {
                                    None
                                }
                            }),
                        }
                    };

                    // create a black key shape
                    let black_key_shape = |octave: i8, black_idx: u8| {
                        let origin = black_key_origin(octave, black_idx);
                        let tone = black_tone(octave, black_idx);
                        PianoKeyShape {
                            origin,
                            size: egui_black_size,
                            corner_size: egui_corner_size,
                            fill_color: highlighted_keys
                                .get(&tone)
                                .map(|&v| {
                                    to_egui_color(
                                        v.map_lightness(|v| v - 0.2)
                                            .with_alpha(black_color.components[3]),
                                    )
                                })
                                .unwrap_or_else(|| egui_black_color),
                            stroke: egui_stroke.clone(),
                            cutoff: [None, None],
                        }
                    };

                    // If the first key is white, then draw it.
                    // In this case the first key's left side doesn't need cutoff.
                    // Returns from which index of white / black keys to start drawing (inclusive).
                    let draw_first_key = || {
                        let otone_start = (tone_start - (o_start - 1) * 12) as u8;
                        if is_black_key_otone(otone_start) {
                            let black_idx = key_idx_of_color(otone_start);
                            let white_idx = black_idx_to_prev_white_idx(black_idx) + 1;

                            (white_idx, black_idx)
                        } else {
                            let white_idx = key_idx_of_color(otone_start);
                            p.add(white_key_shape(o_start - 1, white_idx, [false, true]));
                            let black_idx = white_idx_to_next_black_idx(white_idx);
                            (white_idx + 1, black_idx)
                        }
                    };

                    // If the last key is white, then draw it.
                    // In this case the last key's right side doesn't need cutoff.
                    // Returns from which index of white / black keys to end drawing (not inclusive).
                    let draw_last_key = || {
                        let otone_end = (tone_end - o_end * 12) as u8 - 1;
                        if is_black_key_otone(otone_end) {
                            let black_idx = key_idx_of_color(otone_end);
                            let white_idx = black_idx_to_prev_white_idx(black_idx);

                            (white_idx + 1, black_idx + 1)
                        } else {
                            let white_idx = key_idx_of_color(otone_end);
                            p.add(white_key_shape(o_end, white_idx, [true, false]));
                            let black_idx = white_idx_to_next_black_idx(white_idx);

                            (white_idx, black_idx)
                        }
                    };

                    if o_end < o_start {
                        // all keys within the same octave
                        let (white_idx_start, black_idx_start) = draw_first_key();
                        let (white_idx_end, black_idx_end) = draw_last_key();

                        for white_idx in white_idx_start..white_idx_end {
                            p.add(white_key_shape(o_end, white_idx, [true, true]));
                        }
                        for black_idx in black_idx_start..black_idx_end {
                            p.add(black_key_shape(o_end, black_idx));
                        }
                    } else {
                        // first incomplete octave
                        {
                            let (white_idx_start, black_idx_start) = draw_first_key();

                            for white_idx in white_idx_start..7 {
                                p.add(white_key_shape(o_start - 1, white_idx, [true, true]));
                            }
                            for black_idx in black_idx_start..5 {
                                p.add(black_key_shape(o_start - 1, black_idx));
                            }
                        }

                        // complete octaves
                        for octave in octave_range(key_range) {
                            // white keys
                            for white_idx in 0..7 {
                                p.add(white_key_shape(octave, white_idx, [true, true]));
                            }
                            // black keys
                            for black_idx in 0..5 {
                                p.add(black_key_shape(octave, black_idx));
                            }
                        }

                        // last incomplete octave
                        {
                            let (white_idx_end, black_idx_end) = draw_last_key();

                            for white_idx in 0..white_idx_end {
                                p.add(white_key_shape(o_start - 1, white_idx, [true, true]));
                            }
                            for black_idx in 0..black_idx_end {
                                p.add(black_key_shape(o_start - 1, black_idx));
                            }
                        }
                    }
                }

                // notes
                {
                    let scroll_speed = self.visualizer_config.scroll_speed;
                    let egui_scroll_speed = scroll_speed as f32 * unit;
                    let egui_scroll_height =
                        egui_view_height - egui_status_bar_height - egui_keyboard_height;
                    let ranim_scroll_height = (egui_scroll_height / unit) as f64;
                    let scroll_time = (ranim_scroll_height / scroll_speed * 1e9) as u64;
                    let time_range = self.time..(scroll_time + self.time);
                    let visible_notes = self.music.notes_between_iter(&time_range, key_range);
                    let notes_clip_rect = egui::Rect::from_min_size(
                        egui_view_top_left,
                        egui::vec2(egui_view_width, egui_scroll_height),
                    );
                    let (white_h_scale, black_h_scale) =
                        self.visualizer_config.keyboard_config.size.note_h_scale;

                    let time_to_y = |time: u64| {
                        let y_diff =
                            (time.abs_diff(self.time) as f64 / 1e9) as f32 * egui_scroll_speed;
                        let y_diff = if time < self.time { y_diff } else { -y_diff };
                        y_diff + egui_key_origin.y
                    };

                    let note_rect = |time_range: Range<u64>, note: MultiTrackMidiNote| {
                        let (origin, is_black) = key_origin_and_color(note.key);
                        let y_max = time_to_y(time_range.start);
                        let y_min = time_to_y(time_range.end);
                        let (x_min, x_max) = if is_black {
                            let x_min =
                                origin.x + ((1. - black_h_scale) / 2.) as f32 * egui_black_size.x;
                            let x_max = x_min + black_h_scale as f32 * egui_black_size.x;
                            (x_min, x_max)
                        } else {
                            let x_min =
                                origin.x + ((1. - white_h_scale) / 2.) as f32 * egui_key_unit;
                            let x_max = x_min + white_h_scale as f32 * egui_key_unit;
                            (x_min, x_max)
                        };
                        egui::Rect::from_two_pos(egui::pos2(x_min, y_min), egui::pos2(x_max, y_max))
                    };

                    let note_shape = |time_range: Range<u64>, note: MultiTrackMidiNote| {
                        let rect = note_rect(time_range, note).intersect(notes_clip_rect);
                        let fill_color = {
                            use ColorBy::*;
                            match self.visualizer_config.color_by {
                                Channel => *note_colors.index_cyc(note.loc.channel as usize),
                                Track => *note_colors.index_cyc(note.loc.track as usize),
                                KeyColor => *note_colors.index_cyc(is_black_key(note.key) as usize),
                            }
                        }
                        .with_alpha(note.vel as f32 / 127.);
                        let fill_color = to_egui_color(fill_color);
                        epaint::RectShape::filled(rect, 0., fill_color)
                    };

                    for (time_range, note) in visible_notes {
                        p.add(note_shape(time_range, note));
                    }
                }
            }

            // pedals
            {
                // TODO: paint pedals
            }

            // border rect
            {
                p.rect_stroke(
                    rect,
                    0.,
                    ui.style().visuals.window_stroke(),
                    egui::StrokeKind::Outside,
                );
            }
        })
        .response
    }
}
