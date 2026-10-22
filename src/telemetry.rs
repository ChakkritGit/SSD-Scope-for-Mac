use crate::native::Counters;
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Clone, Debug, Serialize)]
pub struct Sample {
    pub elapsed: f64,
    pub read_mbps: f64,
    pub write_mbps: f64,
}
#[derive(Default)]
pub struct History {
    pub points: VecDeque<Sample>,
    previous: Option<(Counters, f64)>,
}
impl History {
    pub fn push(&mut self, current: Counters, elapsed: f64) {
        if let Some((previous, time)) = &self.previous {
            let dt = elapsed - time;
            if current.registry_id == previous.registry_id
                && dt > 0.0
                && dt <= 15.0
                && let (Some(read), Some(write)) = (
                    current.read_bytes.checked_sub(previous.read_bytes),
                    current.write_bytes.checked_sub(previous.write_bytes),
                )
            {
                self.points.push_back(Sample {
                    elapsed,
                    read_mbps: read as f64 / 1e6 / dt,
                    write_mbps: write as f64 / 1e6 / dt,
                });
            }
        }
        self.previous = Some((current, elapsed));
        while self.points.len() > 120 {
            self.points.pop_front();
        }
    }
}
