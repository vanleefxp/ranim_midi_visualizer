use std::{array, collections::HashMap, ops::Range};

use derive_more::{Deref, DerefMut};
use gpui::*;
use gpui_component::Theme;
use music_utils::{
    KeyInfo, black_idx_to_prev_white_idx, black_tone, is_black_key, is_black_key_otone,
    key_idx_of_color, key_info, octave_range, white_idx_to_next_black_idx, white_tone,
};
use ranim::{
    Output,
    color::{AlphaColor, Srgb},
};

use ranim_midi_visualizer_lib::config::{ColorBy, MetricBase, MidiVisualizerConfig};
use ranim_midi_visualizer_math::cyc_index::IndexCyc as _;
use waveform_utils::music::{Metric, Music, Note, NoteContainer as _};

#[derive(IntoElement)]
pub struct PreviewArea {
    pub music: Entity<Music>,
    pub visualizer_config: Entity<MidiVisualizerConfig>,
    pub export_config: Entity<Output>,
    pub clear_color: Rgba,
    pub time: Metric,
}

fn crop_bounds(b: Bounds<Pixels>, aspect_ratio: f32) -> Bounds<Pixels> {
    let Bounds {
        origin,
        size: Size { width, height },
    } = b;

    let new_width = height * aspect_ratio;
    if width < new_width {
        let new_width = width;
        let new_height = new_width / aspect_ratio;
        let origin_y_offset = (height - new_height) / 2.;
        let new_origin = origin + point(px(0.), origin_y_offset);
        bounds(new_origin, size(new_width, new_height))
    } else {
        let new_height = height;
        let origin_x_offset = (width - new_width) / 2.;
        let new_origin = origin + point(origin_x_offset, px(0.));
        bounds(new_origin, size(new_width, new_height))
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Deref, DerefMut)]
struct RanimColor(AlphaColor<Srgb>);

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

fn key_path(
    path: &mut PathBuilder,
    origin: Point<Pixels>,
    size: Size<Pixels>,
    corner_size: Size<Pixels>,
    cutoff: [Option<Size<Pixels>>; 2],
) -> &mut PathBuilder {
    // Different piano keyboard shapes:
    //
    // 1. cutoff on both sides
    // 2. cutoff on the left side
    // 3. cutoff on the right side
    // 4. no cutoff
    //
    //    1.          2.          3.          4.
    //      ┌──┐        ┌────┐    ┌────┐      ┌──────┐
    //      │  │        │    │    │    │      │      │
    //      │  │        │    │    │    │      │      │
    //      │  │        │    │    │    │      │      │
    //      │  │        │    │    │    │      │      │
    //    ┌─╯  ╰─┐    ┌─╯    │    │    ╰─┐    │      │
    //    │      │    │      │    │      │    │      │
    //    │      │    │      │    │      │    │      │
    //    │      │    │      │    │      │    │      │
    //    │      │    │      │    │      │    │      │
    //    │      │    │      │    │      │    │      │
    //    ╰──────╯    ╰──────╯    ╰──────╯    ╰──────╯

    if let Some(cutoff_left) = cutoff[0] {
        path.move_to(origin + point(cutoff_left.width, px(0.)));
        path.line_to(origin + cutoff_left.into() - point(px(0.), corner_size.height));
        path.curve_to(
            origin + cutoff_left.into() - point(corner_size.width, px(0.)),
            origin + cutoff_left.into(),
        );
        path.line_to(origin + point(px(0.), cutoff_left.height))
    } else {
        path.move_to(origin);
    }
    path.line_to(origin + point(px(0.), size.height - corner_size.height));
    path.curve_to(
        origin + point(corner_size.width, size.height),
        origin + point(px(0.), size.height),
    );
    path.line_to(origin + size.into() - point(corner_size.width, px(0.)));
    path.curve_to(
        origin + size.into() - point(px(0.), corner_size.height),
        origin + size.into(),
    );
    if let Some(cutoff_right) = cutoff[1] {
        path.line_to(origin + point(size.width, cutoff_right.height));
        path.line_to(
            origin
                + point(
                    size.width - cutoff_right.width + corner_size.width,
                    cutoff_right.height,
                ),
        );
        path.curve_to(
            origin
                + point(
                    size.width - cutoff_right.width,
                    cutoff_right.height - corner_size.height,
                ),
            origin + point(size.width, px(0.)) - cutoff_right.into(),
        );
        path.line_to(origin + point(size.width - cutoff_right.width, px(0.)));
    } else {
        path.line_to(origin + point(size.width, px(0.)));
    }
    path.close();

    path
}

impl PreviewArea {
    fn paint_preview(self, view_bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let Self {
            music,
            visualizer_config,
            export_config,
            clear_color,
            time,
        } = self;

        // Crop bounds to fit the video's aspect ratio
        let aspect_ratio = cx.read_entity(&export_config, |v, _cx| {
            (v.width as f64 / v.height as f64) as f32
        });
        let view_bounds = crop_bounds(view_bounds, aspect_ratio);

        // Video background
        window.paint_quad(quad(
            view_bounds,
            Corners::default(),
            solid_background(clear_color),
            Edges::all(px(1.)),
            Theme::global(cx).border,
            BorderStyle::Solid,
        ));

        let ranim_unit = view_bounds.size.height / 8.;
        // let ranim_origin = view_bounds.center();

        cx.read_entity(&visualizer_config, |visualizer_config, cx| {
            cx.read_entity(&music, |music, _cx| {
                let key_range = &visualizer_config.keyboard_config.key_range;
                let keyboard_size = &visualizer_config.keyboard_config.size;
                let keyboard_color = &visualizer_config.keyboard_config.color;
                let status_bar_height = visualizer_config.status_bar_config.height();
                let color_by = visualizer_config.note_config.color_by;
                let note_colors = visualizer_config.note_config.colors.as_slice();

                // Status bar
                let status_bar_bg = visualizer_config.status_bar_config.bg_color;
                let status_bar_height = ranim_unit * status_bar_height as f32;
                let status_bar_origin =
                    view_bounds.bottom_left() - point(px(0.), status_bar_height);
                let status_bar_bounds = bounds(status_bar_origin, size(px(0.), status_bar_height));
                window.paint_layer(status_bar_bounds, |window| {
                    window.paint_quad(quad(
                        status_bar_bounds,
                        0.,
                        solid_background(RanimColor(status_bar_bg)),
                        Edges::default(),
                        transparent_black(),
                        BorderStyle::Solid,
                    ));
                });

                let highlighted_keys: HashMap<_, _> = {
                    use ColorBy::*;
                    let time_range = time..=time;
                    let mapped_music = music.as_mapped();
                    let notes_on =
                        mapped_music
                            .notes_overlaps(time_range)
                            .filter_map(|(pos, _, note)| {
                                if key_range.contains(&note.pitch) {
                                    Some((note.pitch, pos))
                                } else {
                                    None
                                }
                            });
                    match color_by {
                        Voice => notes_on
                            .map(|(pitch, [_, voice_idx])| {
                                (pitch, *note_colors.index_cyc(voice_idx))
                            })
                            .collect(),
                        Staff => notes_on
                            .map(|(pitch, [staff_idx, _])| {
                                (pitch, *note_colors.index_cyc(staff_idx))
                            })
                            .collect(),
                        KeyColor => notes_on
                            .map(|(pitch, _)| {
                                (pitch, *note_colors.index_cyc(is_black_key(pitch) as usize))
                            })
                            .collect(),
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

                let white_key_width = view_bounds.size.width / (right - left) as f32;
                let keyboard_height = white_key_width * keyboard_size.white_height as f32;
                let key_origin = view_bounds.bottom_left()
                    - point(
                        left as f32 * white_key_width,
                        keyboard_height + status_bar_height,
                    );
                let key_bounds = bounds(
                    point(view_bounds.origin.x, key_origin.y),
                    size(view_bounds.size.width, keyboard_height),
                );

                let white_key_size = size(white_key_width, keyboard_height);
                let black_key_size = size(
                    white_key_width * keyboard_size.black_size.x as f32,
                    white_key_width * keyboard_size.black_size.y as f32,
                );
                let corner_size = size(
                    white_key_width * keyboard_size.corner_size.x as f32,
                    white_key_width * keyboard_size.corner_size.y as f32,
                );
                let [white_color, black_color] = keyboard_color.key_color.map(RanimColor);
                let stroke_color = RanimColor(keyboard_color.stroke_color);
                let stroke_width = ranim_unit * visualizer_config.keyboard_config.stroke_width.0;

                let overlaps = keyboard_size.white_key_overlap_widths().map(|v| {
                    v.map(|v| {
                        if v <= 0. {
                            None
                        } else {
                            Some(size(v as f32 * white_key_width, black_key_size.height))
                        }
                    })
                });

                let white_key_origin = |octave: i8, white_idx: u8| {
                    key_origin
                        + point(
                            white_key_width * (white_idx as f32 + octave as f32 * 7.),
                            px(0.),
                        )
                };
                let black_key_origin = |octave: i8, black_idx: u8| {
                    let white_idx = black_idx_to_prev_white_idx(black_idx);
                    let disp = keyboard_size.black_offset[black_idx as usize];
                    key_origin
                        + point(
                            white_key_width * (white_idx as f32 + octave as f32 * 7. + 1.)
                                + black_key_size.width * (disp - 1.) as f32 / 2.,
                            px(0.),
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
                window.paint_layer(key_bounds, |window| {
                    let paint_white_key =
                        |window: &mut Window, octave: i8, white_idx: u8, cutoff_mask: [bool; 2]| {
                            let origin = white_key_origin(octave, white_idx);
                            let pitch = white_tone(octave, white_idx);
                            let [fill_path, stroke_path] =
                                [PathBuilder::fill(), PathBuilder::stroke(stroke_width)].map(
                                    |mut pb| {
                                        key_path(
                                            &mut pb,
                                            origin,
                                            white_key_size,
                                            corner_size,
                                            array::from_fn(|i| {
                                                if cutoff_mask[i] {
                                                    overlaps[white_idx as usize][i]
                                                } else {
                                                    None
                                                }
                                            }),
                                        );
                                        pb.build().expect("Failed to build path")
                                    },
                                );
                            let key_color = highlighted_keys
                                .get(&pitch)
                                .map(|v| RanimColor(v.with_alpha(white_color.components[3])))
                                .unwrap_or(white_color);
                            window.paint_path(fill_path, solid_background(key_color));
                            window.paint_path(stroke_path, solid_background(stroke_color));
                        };
                    let paint_black_key = |window: &mut Window, octave: i8, black_idx: u8| {
                        let origin = black_key_origin(octave, black_idx);
                        let pitch = black_tone(octave, black_idx);
                        let [fill_path, stroke_path] =
                            [PathBuilder::fill(), PathBuilder::stroke(stroke_width)].map(
                                |mut pb| {
                                    key_path(
                                        &mut pb,
                                        origin,
                                        black_key_size,
                                        corner_size,
                                        [None; 2],
                                    );
                                    pb.build().expect("Failed to build path")
                                },
                            );
                        let key_color = highlighted_keys
                            .get(&pitch)
                            .map(|v| {
                                RanimColor(
                                    v.with_alpha(black_color.components[3])
                                        .map_lightness(|v| (v - 0.2).max(0.)),
                                )
                            })
                            .unwrap_or(black_color);
                        window.paint_path(fill_path, solid_background(key_color));
                        window.paint_path(stroke_path, solid_background(stroke_color));
                    };

                    // If the first key is white, then draw it.
                    // In this case the first key's left side doesn't need cutoff.
                    // Returns from which index of white / black keys to start drawing (inclusive).
                    let paint_first_key = |window: &mut Window| {
                        let otone_start = (tone_start - (o_start - 1) * 12) as u8;
                        if is_black_key_otone(otone_start) {
                            let black_idx = key_idx_of_color(otone_start);
                            let white_idx = black_idx_to_prev_white_idx(black_idx) + 1;

                            (white_idx, black_idx)
                        } else {
                            let white_idx = key_idx_of_color(otone_start);
                            paint_white_key(window, o_start - 1, white_idx, [false, true]);
                            let black_idx = white_idx_to_next_black_idx(white_idx);
                            (white_idx + 1, black_idx)
                        }
                    };

                    // If the last key is white, then draw it.
                    // In this case the last key's right side doesn't need cutoff.
                    // Returns from which index of white / black keys to end drawing (not inclusive).
                    let paint_last_key = |window: &mut Window| {
                        let otone_end = (tone_end - o_end * 12) as u8 - 1;
                        if is_black_key_otone(otone_end) {
                            let black_idx = key_idx_of_color(otone_end);
                            let white_idx = black_idx_to_prev_white_idx(black_idx);

                            (white_idx + 1, black_idx + 1)
                        } else {
                            let white_idx = key_idx_of_color(otone_end);
                            paint_white_key(window, o_end, white_idx, [true, false]);
                            let black_idx = white_idx_to_next_black_idx(white_idx);

                            (white_idx, black_idx)
                        }
                    };

                    if o_end < o_start {
                        // all keys within the same octave
                        let (white_idx_start, black_idx_start) = paint_first_key(window);
                        let (white_idx_end, black_idx_end) = paint_last_key(window);

                        for white_idx in white_idx_start..white_idx_end {
                            paint_white_key(window, o_end, white_idx, [true, true]);
                        }
                        for black_idx in black_idx_start..black_idx_end {
                            paint_black_key(window, o_end, black_idx);
                        }
                    } else {
                        // first incomplete octave
                        {
                            let (white_idx_start, black_idx_start) = paint_first_key(window);

                            for white_idx in white_idx_start..7 {
                                paint_white_key(window, o_start - 1, white_idx, [true, true]);
                            }
                            for black_idx in black_idx_start..5 {
                                paint_black_key(window, o_start - 1, black_idx);
                            }
                        }

                        // complete octaves
                        for octave in octave_range(key_range) {
                            // white keys
                            for white_idx in 0..7 {
                                paint_white_key(window, octave, white_idx, [true, true]);
                            }
                            // black keys
                            for black_idx in 0..5 {
                                paint_black_key(window, octave, black_idx);
                            }
                        }

                        // last incomplete octave
                        {
                            let (white_idx_end, black_idx_end) = paint_last_key(window);

                            for white_idx in 0..white_idx_end {
                                paint_white_key(window, o_start - 1, white_idx, [true, true]);
                            }
                            for black_idx in 0..black_idx_end {
                                paint_black_key(window, o_start - 1, black_idx);
                            }
                        }
                    }
                });

                // Notes
                let scroll_speed: f64 = visualizer_config.scroll_speed.into();
                let metric_base = visualizer_config.metric_base;
                let scroll_bounds = bounds(
                    view_bounds.origin,
                    size(view_bounds.size.width, key_origin.y - view_bounds.origin.y),
                );
                let scroll_speed = ranim_unit * scroll_speed as f32;
                let [white_h_scale, black_h_scale] = visualizer_config.note_config.h_scale;

                let note_x_bounds = |note: Note| {
                    let (origin, is_black) = key_origin_and_color(note.pitch);
                    let (note_width, h_scale): (Pixels, f64) = if is_black {
                        (black_key_size.width, black_h_scale.into())
                    } else {
                        (white_key_size.width, white_h_scale.into())
                    };
                    let note_width = note_width * h_scale as f32;
                    let note_origin_x = origin.x + px((1. - h_scale / 2.) as f32);
                    (note_origin_x, note_width)
                };

                macro paint_notes($window: expr, $note_container: expr, $cur_metric: expr$(,)?) {
                    let to_large_metric =
                        |time_unit: Metric| time_unit as f64 / music.time_resolution.get() as f64;
                    let to_metric = |large_metric: f64| {
                        (large_metric * music.time_resolution.get() as f64) as Metric
                    };

                    let scroll_large_metric = (scroll_bounds.size.height / scroll_speed) as f64;
                    let scroll_metric = to_metric(scroll_large_metric);
                    let time_unit_range = $cur_metric..=($cur_metric + scroll_metric);
                    let note_container = $note_container;
                    let visible_notes = note_container
                        .notes_overlaps(time_unit_range)
                        .filter(|(_, _, note)| key_range.contains(&note.pitch));

                    let metric_to_y = |metric: Metric| {
                        let y_diff = to_large_metric($cur_metric - metric) as f32 * scroll_speed;
                        y_diff + key_origin.y
                    };

                    let note_y_bounds = |time_range: Range<Metric>| {
                        let note_height = to_large_metric(time_range.end - time_range.start) as f32
                            * scroll_speed;
                        let note_origin_y = metric_to_y(time_range.end);
                        (note_origin_y, note_height)
                    };

                    let note_bounds = |time_range: Range<Metric>, note: Note| {
                        let (origin_x, width) = note_x_bounds(note);
                        let (origin_y, height) = note_y_bounds(time_range);
                        bounds(point(origin_x, origin_y), size(width, height))
                            .intersect(&scroll_bounds)
                    };

                    for ([staff_idx, voice_idx], time_range, &note) in visible_notes {
                        use ColorBy::*;
                        let note_color = RanimColor(
                            match visualizer_config.note_config.color_by {
                                Voice => *note_colors.index_cyc(voice_idx),
                                Staff => *note_colors.index_cyc(staff_idx),
                                KeyColor => {
                                    *note_colors.index_cyc(is_black_key(note.pitch) as usize)
                                }
                            }
                            .with_alpha(f64::from(note.velocity) as f32),
                        );
                        $window.paint_quad(quad(
                            note_bounds(time_range, note),
                            Corners::default(),
                            solid_background(note_color),
                            Edges::default(),
                            transparent_black(),
                            BorderStyle::Solid,
                        ));
                    }
                }

                window.paint_layer(scroll_bounds, |window| {
                    use MetricBase::*;
                    match metric_base {
                        Time => {
                            paint_notes!(window, music.as_mapped(), time);
                        }
                        Beat => {
                            let cur_tick = music.time_map().eval_inv(&time, true);
                            paint_notes!(window, music.as_unmapped(), cur_tick);
                        }
                    }
                });
            });
        });
    }
}

impl RenderOnce for PreviewArea {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        canvas(
            |_bounds, _window, _cx| {},
            |bounds, _, window, cx| {
                self.paint_preview(bounds, window, cx);
            },
        )
        .size_full()
    }
}
