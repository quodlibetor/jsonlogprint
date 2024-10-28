use owo_colors::OwoColorize;
use owo_colors::Style;
use supports_color::Stream;
use std::fmt;

use crate::ColorOption;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Styler {
    pub(crate) colorize: bool,
}

impl Styler {
    pub(crate) fn new(when: ColorOption) -> Self {
        let colorize = match when {
            ColorOption::Always => true,
            ColorOption::Auto => supports_color::on(Stream::Stdout).is_some() || std::env::var("CI").is_ok(),
            ColorOption::Never => false,
        };
        Self { colorize }
    }

    pub(crate) fn timestamp<D: fmt::Display>(self, timestamp: &D) -> TimestampDisplay<'_, D> {
        TimestampDisplay(self, timestamp)
    }

    pub(crate) fn level(self, level: &str) -> LevelDisplay<'_> {
        LevelDisplay(self, level)
    }

    pub(crate) fn depth(self, val: &str, depth: usize) -> DepthDisplay<'_> {
        DepthDisplay {
            styler: self,
            val,
            depth,
        }
    }

    fn timestamp_style(&self) -> Style {
        if !self.colorize {
            return Style::new();
        }
        Style::new().dimmed()
    }

    fn depth_style(&self, depth: usize) -> Style {
        if !self.colorize {
            return Style::new();
        }
        match depth % 6 {
            0 => Style::new().blue(),
            1 => Style::new().cyan(),
            2 => Style::new().green(),
            3 => Style::new().blue().dimmed(),
            4 => Style::new().cyan().dimmed(),
            5 => Style::new().green().dimmed(),
            _ => Style::new(),
        }
    }

    fn level_style(&self, level: &str) -> Style {
        if !self.colorize {
            return Style::new();
        }
        use unicase::Ascii;
        let level = Ascii::new(level);
        if level == Ascii::new("crit") || level == Ascii::new("critical") {
            Style::new().red().bold()
        } else if level == Ascii::new("error") {
            Style::new().red()
        } else if level == Ascii::new("warn") || level == Ascii::new("warning") {
            Style::new().yellow()
        } else if level == Ascii::new("info") {
            Style::new().cyan()
        } else if level == Ascii::new("debug") {
            Style::new().blue().dimmed()
        } else if level == Ascii::new("trace") {
            Style::new().dimmed()
        } else {
            Style::new()
        }
    }
}

pub(crate) struct TimestampDisplay<'a, D: fmt::Display>(Styler, &'a D);

impl<'a, D: fmt::Display> fmt::Display for TimestampDisplay<'a, D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.1.style(self.0.timestamp_style()))
    }
}

pub(crate) struct LevelDisplay<'a>(Styler, &'a str);

impl<'a> fmt::Display for LevelDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.1.style(self.0.level_style(self.1)))
    }
}

pub(crate) struct DepthDisplay<'a> {
    styler: Styler,
    val: &'a str,
    depth: usize,
}

impl<'a> fmt::Display for DepthDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.val.style(self.styler.depth_style(self.depth)))
    }
}
