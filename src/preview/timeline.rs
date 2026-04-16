pub struct TimelineState {
    pub total_sec: f64,
    pub current_sec: f64,
    // pub width_sec: f64,
    // pub offset_points: f32,
}

#[allow(unused)]
impl TimelineState {
    pub fn new(total_sec: f64) -> Self {
        Self {
            total_sec,
            current_sec: 0.0,
            // width_sec: total_sec,
            // offset_points: 0.0,
        }
    }
}
