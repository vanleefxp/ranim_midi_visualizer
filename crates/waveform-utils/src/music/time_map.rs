use std::num::NonZero;

use ranim_midi_visualizer_math::func::{LadderFn, SegmentedLinearFn};

use crate::music::{Metric, Tempo};

pub type TimeMap = SegmentedLinearFn<Metric, Metric>;

pub(crate) fn generate_time_map(
    tempo: &LadderFn<Metric, Tempo>,
    beat_resolution: NonZero<Metric>,
    time_resolution: NonZero<Metric>,
) -> SegmentedLinearFn<Metric, Metric> {
    tempo
        .iter()
        .scan(
            (0u64, 0u64, time_resolution),
            |(cur_time, cur_tick, cur_tempo), (&tick, &tempo)| {
                let point = (*cur_tick, *cur_time);

                let n_beat = (tick - *cur_tick) as f64 / beat_resolution.get() as f64;
                let n_time_units = (cur_tempo.get() as f64 * n_beat) as Metric;

                *cur_time += n_time_units;
                *cur_tick = tick;
                *cur_tempo = tempo;
                Some(point)
            },
        )
        .collect()
}
