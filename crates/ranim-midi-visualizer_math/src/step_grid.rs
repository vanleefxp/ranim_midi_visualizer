#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoundMode {
    #[default]
    Floor,
    Ceil,
    Round,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepGrid {
    pub len: usize,
    pub steps: usize,
    pub round_mode: RoundMode,
}

impl StepGrid {
    pub fn idx_to_step(&self, idx: usize) -> usize {
        use RoundMode::*;
        let val = self.steps as f64 / self.len as f64 * idx as f64;
        match self.round_mode {
            Floor => val.floor() as usize,
            Ceil => val.ceil() as usize,
            Round => val.round() as usize,
        }
    }

    pub fn step_to_idx(&self, step: usize) -> usize {
        use RoundMode::*;
        let val = step as f64 / self.steps as f64 * self.len as f64;
        match self.round_mode {
            Floor => val.floor() as usize,
            Ceil => val.ceil() as usize,
            Round => val.round() as usize,
        }
    }
}

impl IntoIterator for StepGrid {
    type Item = usize;
    type IntoIter = StepGridIter;

    fn into_iter(self) -> Self::IntoIter {
        StepGridIter {
            grid: self,
            idx_start: 0,
            idx_end: self.len,
        }
    }
}

pub struct StepGridIter {
    grid: StepGrid,
    idx_start: usize,
    idx_end: usize,
}

impl Iterator for StepGridIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let value = if self.idx_start < self.idx_end {
            Some(self.grid.idx_to_step(self.idx_start))
        } else {
            None
        };
        self.idx_start += 1;

        value
    }
}

impl ExactSizeIterator for StepGridIter {
    fn len(&self) -> usize {
        self.grid.len
    }
}

impl DoubleEndedIterator for StepGridIter {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.idx_end <= self.idx_start {
            return None;
        }
        self.idx_end -= 1;
        Some(self.grid.idx_to_step(self.idx_end))
    }
}
