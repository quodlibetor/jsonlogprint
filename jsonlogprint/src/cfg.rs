use chrono::format::Item;
use chrono::format::StrftimeItems;
use clap::{Parser, ValueEnum};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Args {
    /// Fields to print at the beginning of the log line without a key prefix
    #[arg(
        short,
        long,
        value_delimiter = ',',
        default_value = "time,timestamp,ts,level,msg,message"
    )]
    pub(crate) no_key_fields: Vec<String>,

    /// Color output settings: always, auto, never
    #[arg(long, value_enum, default_value = "auto")]
    pub(crate) color: ColorOption,

    /// Timestamp format.
    ///
    /// Auto, Seconds or Millis will be converted to ISO format in output,
    /// Raw means it is not processed.
    #[arg(long, visible_alias = "tsfmt", value_enum, default_value = "auto")]
    pub(crate) timestamp_format: TimestampFormat,

    /// The field to use as the timestamp.
    ///
    /// If the field is an integer, it will be parsed according to --timestamp-format
    #[arg(long, default_value = "timestamp")]
    pub(crate) timestamp_field: String,

    /// The field to use as the log level.
    /// If the field is a string, it will be colorized.
    #[arg(long, default_value = "level")]
    pub(crate) level_field: String,
}

#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) no_key_fields: Vec<String>,
    pub(crate) color: ColorOption,
    pub(crate) timestamp_format: TimestampFormat,
    pub(crate) timestamp_field: String,
    pub(crate) level_field: String,
    pub(crate) millis_out_format: Vec<Item<'static>>,
    pub(crate) secs_out_format: Vec<Item<'static>>,
}

impl Config {
    pub(crate) fn new(args: Args) -> Self {
        Self {
            no_key_fields: args.no_key_fields,
            color: args.color,
            timestamp_format: args.timestamp_format,
            timestamp_field: args.timestamp_field,
            level_field: args.level_field,
            millis_out_format: default_millis_out_format(),
            secs_out_format: default_secs_out_format(),
        }
    }
}

pub(crate) fn default_millis_out_format() -> Vec<Item<'static>> {
    StrftimeItems::new("%Y-%m-%dT%H:%M:%S.%3fZ")
        .parse()
        .unwrap()
}
pub(crate) fn default_secs_out_format() -> Vec<Item<'static>> {
    StrftimeItems::new("%Y-%m-%dT%H:%M:%SZ").parse().unwrap()
}

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ColorOption {
    Always,
    Auto,
    Never,
}

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TimestampFormat {
    Auto,
    Seconds,
    Millis,
    Raw,
}
