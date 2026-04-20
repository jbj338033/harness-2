use crate::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::time::Duration;

pub struct Steps {
    mp: MultiProgress,
    bars: Vec<ProgressBar>,
}

impl Steps {
    #[must_use]
    pub fn new(header: impl AsRef<str>) -> Self {
        let bp = style::bold_primary();
        println!("{bp}◆ {}{bp:#}", header.as_ref());
        Self {
            mp: MultiProgress::new(),
            bars: Vec::new(),
        }
    }

    pub fn add(&mut self, label: impl Into<String>) -> usize {
        let bar = self.mp.add(ProgressBar::new_spinner());
        bar.set_style(pending_style());
        bar.set_message(label.into());
        bar.tick();
        self.bars.push(bar);
        self.bars.len() - 1
    }

    pub fn start(&self, idx: usize) {
        if let Some(bar) = self.bars.get(idx) {
            bar.set_style(active_style());
            bar.enable_steady_tick(Duration::from_millis(80));
        }
    }

    pub fn ok(&self, idx: usize) {
        if let Some(bar) = self.bars.get(idx) {
            bar.disable_steady_tick();
            bar.set_style(done_style());
            bar.tick();
            bar.finish();
        }
    }

    pub fn fail(&self, idx: usize, err: &str) {
        if let Some(bar) = self.bars.get(idx) {
            bar.disable_steady_tick();
            bar.set_style(failed_style());
            let label = bar.message();
            bar.finish_with_message(format!("{label} ({err})"));
        }
    }

    pub fn ok_message(&self, idx: usize, msg: impl Into<String>) {
        if let Some(bar) = self.bars.get(idx) {
            bar.disable_steady_tick();
            bar.set_style(done_style());
            bar.set_message(msg.into());
            bar.tick();
            bar.finish();
        }
    }
}

fn pending_style() -> ProgressStyle {
    let dim = style::dim();
    let tmpl = format!("  {dim}·{dim:#} {{msg}}");
    ProgressStyle::with_template(&tmpl).expect("static template")
}

fn active_style() -> ProgressStyle {
    let primary = style::primary();
    let tmpl = format!("  {primary}{{spinner}}{primary:#} {{msg}}");
    ProgressStyle::with_template(&tmpl)
        .expect("static template")
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
}

fn done_style() -> ProgressStyle {
    let primary = style::primary();
    let tmpl = format!("  {primary}✓{primary:#} {{msg}}");
    ProgressStyle::with_template(&tmpl).expect("static template")
}

fn failed_style() -> ProgressStyle {
    let err = style::err();
    let tmpl = format!("  {err}✗{err:#} {{msg}}");
    ProgressStyle::with_template(&tmpl).expect("static template")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_assigns_sequential_indices() {
        let mut s = Steps::new("test");
        let a = s.add("first");
        let b = s.add("second");
        assert_eq!(a, 0);
        assert_eq!(b, 1);
    }

    #[test]
    fn templates_construct() {
        let _ = pending_style();
        let _ = active_style();
        let _ = done_style();
        let _ = failed_style();
    }
}
