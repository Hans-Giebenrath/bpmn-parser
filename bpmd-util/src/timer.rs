extern crate alloc;

use alloc::{boxed::Box, string::String, vec::Vec};
use core::time::Duration;

pub struct Timer {
    measurements: Vec<(&'static str, Duration)>,
    now: Box<dyn Fn() -> Duration>,
    elapsed: Box<dyn Fn(Duration, Duration) -> Duration>,
    print: Box<dyn Fn(&str)>,
}

impl Timer {
    pub fn new(
        now: Box<dyn Fn() -> Duration>,
        elapsed: Box<dyn Fn(Duration, Duration) -> Duration>,
        print: Box<dyn Fn(&str)>,
    ) -> Self {
        Self {
            measurements: Vec::new(),
            now,
            elapsed,
            print,
        }
    }

    pub fn time_it<F, R>(&mut self, label: &'static str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        (self.print)(label);

        let start = (self.now)();
        let r = f();
        let end = (self.now)();

        self.measurements.push((label, (self.elapsed)(start, end)));

        r
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if self.measurements.is_empty() {
            return;
        }

        let longest_label = self
            .measurements
            .iter()
            .map(|(label, _)| label.len())
            .max()
            .unwrap();

        for (label, duration) in &self.measurements {
            use alloc::format;

            let padding: String = core::iter::repeat_n(' ', longest_label - label.len()).collect();

            let line = format!(
                "{padding}{label} took {:7.2} ms",
                duration.as_micros() as f64 / 1000.
            );

            (self.print)(&line);
        }
    }
}
