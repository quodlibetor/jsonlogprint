use owo_colors::OwoColorize;
use owo_colors::Style;
use owo_colors::StyledList;
use std::fmt;
use supports_color::Stream;

use crate::cfg::ColorOption;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Styler {
    pub(crate) colorize: bool,
}

impl Styler {
    pub(crate) fn new(when: ColorOption) -> Self {
        let colorize = match when {
            ColorOption::Always => true,
            ColorOption::Auto => {
                supports_color::on(Stream::Stdout).is_some() || std::env::var("CI").is_ok()
            }
            ColorOption::Never => false,
        };
        Self { colorize }
    }

    pub(crate) fn empty(self) -> CustomDisplay<'static> {
        CustomDisplay {
            styler: self,
            style: DisplayStyle::Empty,
            value: "",
        }
    }

    pub(crate) fn timestamp<D: fmt::Display>(self, timestamp: &D) -> TimestampDisplay<'_, D> {
        TimestampDisplay(self, timestamp)
    }

    pub(crate) fn level(self, level: &str) -> CustomDisplay<'_> {
        CustomDisplay {
            styler: self,
            style: DisplayStyle::Level,
            value: level,
        }
    }

    pub(crate) fn depth(self, val: &str, depth: usize) -> CustomDisplay<'_> {
        CustomDisplay {
            styler: self,
            style: DisplayStyle::Depth(depth as u16),
            value: val,
        }
    }

    pub(crate) fn depth_multi<'a>(
        self,
        value: &'a str,
        extra: &'a str,
        depth: usize,
    ) -> CustomDisplay<'a> {
        CustomDisplay {
            styler: self,
            style: DisplayStyle::DepthMulti(depth as u16, extra),
            value,
        }
    }

    fn timestamp_style(&self) -> Style {
        if !self.colorize {
            return Style::new();
        }
        Style::new().dimmed()
    }

    fn depth_style(&self, depth: u16) -> Style {
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

enum DisplayStyle<'a> {
    Empty,
    Depth(u16),
    DepthMulti(u16, &'a str),
    Level,
}

pub(crate) struct CustomDisplay<'a> {
    styler: Styler,
    style: DisplayStyle<'a>,
    value: &'a str,
}

impl<'a> fmt::Display for CustomDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.style {
            DisplayStyle::Empty => Ok(()),
            DisplayStyle::Depth(depth) => {
                write!(f, "{}", self.value.style(self.styler.depth_style(depth)))
            }
            DisplayStyle::DepthMulti(depth, second) => {
                let key = self.value.style(self.styler.depth_style(depth));
                let key2 = second.style(self.styler.depth_style(depth));
                write!(f, "{}", StyledList::from([key, key2]))
            }
            DisplayStyle::Level => write!(
                f,
                "{}",
                self.value.style(self.styler.level_style(self.value))
            ),
        }
    }
}

// TODO: Maybe move this into DisplayStyle? makes it uglier and it's not necessary now
pub(crate) struct TimestampDisplay<'a, D: fmt::Display>(Styler, &'a D);

impl<'a, D: fmt::Display> fmt::Display for TimestampDisplay<'a, D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.1.style(self.0.timestamp_style()))
    }
}
