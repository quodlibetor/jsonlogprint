use chrono::{DateTime, Utc};
use clap::{Parser, ValueEnum};
use indexmap::IndexMap;
use serde::de::DeserializeSeed as _;
use std::io::{self, BufRead, Write};
use tracing::{debug, trace, warn};
use tracing_subscriber::{self, EnvFilter};

use deser::JsonValue;

use self::styler::Styler;

mod deser;
mod styler;

/// The number of seconds between 1970 and 3000
///
/// If timestamp_format = auto we use this to determine if we should convert
/// using millis or seconds.
const YEAR_3K_EPOCH: i64 = 32503698000;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Fields to print at the beginning of the log line without a key prefix
    #[arg(
        short,
        long,
        value_delimiter = ',',
        default_value = "timestamp,ts,level,msg,message"
    )]
    no_key_fields: Vec<String>,

    /// Color output settings: always, auto, never
    #[arg(long, value_enum, default_value = "auto")]
    color: ColorOption,

    /// Timestamp format.
    ///
    /// Auto, Seconds or Millis will be converted to ISO format in output,
    /// Raw means it is not processed.
    #[arg(long, visible_alias = "tsfmt", value_enum, default_value = "auto")]
    timestamp_format: TimestampFormat,

    /// The field to use as the timestamp.
    ///
    /// If the field is an integer, it will be parsed according to --timestamp-format
    #[arg(long, default_value = "timestamp")]
    timestamp_field: String,

    /// The field to use as the log level.
    /// If the field is a string, it will be colorized.
    #[arg(long, default_value = "level")]
    level_field: String,
}

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
enum ColorOption {
    Always,
    Auto,
    Never,
}

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
enum TimestampFormat {
    Auto,
    Seconds,
    Millis,
    Raw,
}


fn main() {
    let args = Args::parse();

    init_logging();
    debug!(config = ?args, "starting up");

    let stdin = io::stdin();
    let handle = stdin.lock();
    let stdout = io::stdout();
    let handle_out = stdout.lock();

    transform_lines(handle, handle_out, args);
}

fn init_logging() {
    static INIT: std::sync::Once = std::sync::Once::new();

    INIT.call_once(|| {
        let default_filter = std::env::var("JLP_LOG_FILTER").unwrap_or_else(|_| {
            if cfg!(test) {
                "trace".to_string() // Use debug level for tests
            } else {
                "warn".to_string()
            }
        });
        let env_filter = EnvFilter::new(default_filter);
        let builder = tracing_subscriber::fmt().with_env_filter(env_filter);
        if cfg!(test) {
            builder.with_test_writer().init();
        } else {
            builder.init();
        }
    });
}

fn transform_lines(handle: impl BufRead, mut out: impl Write, args: Args) {
    // Reuse the same map for each line
    let mut reusable_map = IndexMap::new();
    let styler = Styler::new(args.color );

    for line in handle.lines() {
        match line {
            Ok(json_line) => {
                let deserializer = &mut serde_json::Deserializer::from_str(&json_line);

                // Deserialize directly into our reusable map using the custom seed
                let seed = deser::IndexMapSeed {
                    map: &mut reusable_map,
                };

                match seed.deserialize(deserializer) {
                    Ok(()) => {
                        if let Err(e) = json_to_logfmt(
                            &mut reusable_map,
                            &mut out,
                            &args.no_key_fields,
                            &args.timestamp_format,
                            &args.timestamp_field,
                            &args.level_field,
                            styler,
                        ) {
                            debug!("Failed to format JSON line: {}", e);
                            writeln!(out).unwrap();
                            writeln!(out, "{}", json_line).unwrap();
                        }
                        writeln!(out).unwrap();
                    }
                    Err(e) => {
                        debug!(
                            "Failed to deserialize JSON line: {} with error: {}",
                            json_line, e
                        );
                        writeln!(out, "{}", json_line).unwrap();
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read line from stdin: {}", e);
                writeln!(out).unwrap();
            }
        }
    }
}

fn json_to_logfmt(
    map: &mut IndexMap<String, JsonValue>,
    out: &mut impl Write,
    no_key_fields: &[String],
    timestamp_format: &TimestampFormat,
    timestamp_field: &str,
    level_field: &str,
    styler: Styler,
) -> io::Result<()> {
    let mut first = true;
    // Print fields specified in no_key_fields first if they exist
    for key in no_key_fields {
        if let Some(value) = map.get_mut(key) {
            if !first {
                write!(out, " ")?;
            } else {
                first = false;
            }
            match value {
                JsonValue::String(val_str) => {
                    if key == level_field {
                        write!(out, "{}", styler.level(val_str))?;
                    } else {
                        write!(out, "{}", val_str)?;
                    }
                }
                JsonValue::Number(num) => {
                    if key == timestamp_field {
                        let timestamp = num.as_i64().unwrap_or_default();
                        if *timestamp_format != TimestampFormat::Raw {
                            // overwrite this if format is auto
                            let mut tsfmt = *timestamp_format;
                            let iso_datetime = match timestamp_format {
                                TimestampFormat::Auto if timestamp > YEAR_3K_EPOCH => {
                                    tsfmt = TimestampFormat::Millis;
                                    DateTime::<Utc>::from_timestamp(
                                        timestamp / 1000,
                                        (timestamp % 1000 * 1_000_000) as u32,
                                    )
                                }
                                TimestampFormat::Auto => {
                                    tsfmt = TimestampFormat::Seconds;
                                    DateTime::<Utc>::from_timestamp(timestamp, 0)
                                }
                                TimestampFormat::Seconds => {
                                    DateTime::<Utc>::from_timestamp(timestamp, 0)
                                }
                                TimestampFormat::Millis => DateTime::<Utc>::from_timestamp(
                                    timestamp / 1000,
                                    (timestamp % 1000 * 1_000_000) as u32,
                                ),
                                TimestampFormat::Raw => unreachable!(),
                            };

                            match (iso_datetime, tsfmt) {
                                (Some(dt), TimestampFormat::Seconds) => {
                                    write!(
                                        out,
                                        "{}",
                                        styler.timestamp(&dt.format("%Y-%m-%dT%H:%M:%SZ"))
                                    )
                                    .unwrap();
                                }
                                (Some(dt), TimestampFormat::Millis) => {
                                    write!(
                                        out,
                                        "{}",
                                        styler.timestamp(&dt.format("%Y-%m-%dT%H:%M:%S.%3fZ"))
                                    )
                                    .unwrap();
                                }
                                _ => {
                                    write!(out, "{}", timestamp)?;
                                }
                            }
                        } else {
                            write!(out, "{}", timestamp)?;
                        }
                    } else {
                        write!(out, "{}", num)?;
                    }
                }
                _ => continue,
            }
            *value = JsonValue::Removed;
        }
    }

    // Print the rest of the fields, excluding Removed variants
    let iter_keys: Vec<String> = map.keys().cloned().collect();
    let mut newline_fields: Vec<(String, &JsonValue)> = Vec::new();
    for key in iter_keys {
        if let Some(value) = map.get(&key) {
            match value {
                JsonValue::Removed => continue,
                JsonValue::String(val_str) if val_str.contains('\n') => {
                    newline_fields.push((key, value));
                    continue;
                }
                _ => {
                    if !first {
                        write!(out, " ").unwrap();
                    }
                    display_value_recursive(out, value, &key, 0, styler)?;
                    first = false;
                }
            }
        }
    }

    // Print fields containing newlines at the end
    for (key, value) in newline_fields {
        writeln!(out).unwrap();
        display_value_recursive(out, value, &key, 0, styler)?;
    }

    Ok(())
}

fn display_value_recursive(
    out: &mut impl Write,
    value: &JsonValue,
    prefix: &str,
    depth: usize,
    styler: Styler,
) -> io::Result<()> {
    trace!(?value, ?depth, "display_value_recursive");
    let colored_prefix = styler.depth(prefix, depth);
    let braces = styler.depth("{", depth);
    let braces_end = styler.depth("}", depth);

    match value {
        JsonValue::String(s) => {
            if s.contains(' ') || s.contains('"') || s.contains('\\') {
                let val = s.replace('\\', r"\\").replace('"', r#"\""#);
                write!(out, r#"{colored_prefix}="{val}""#)
            } else {
                write!(out, "{colored_prefix}={s}")
            }
        }
        JsonValue::Number(n) => write!(out, "{colored_prefix}={n}"),
        JsonValue::Bool(b) => write!(out, "{colored_prefix}={b}"),
        JsonValue::Null => write!(out, "{colored_prefix}=null"),
        JsonValue::Removed => Ok(()), // This won't be used since Removed values are skipped
        JsonValue::Object(map) => {
            write!(out, "{colored_prefix}{braces}")?;
            let mut first = true;
            for (key, val) in map.iter() {
                if !first {
                    write!(out, " ")?;
                } else {
                    first = false;
                }
                display_value_recursive(out, val, key, depth + 1, styler)?
            }
            write!(out, "{braces_end}")?;
            Ok(())
        }
        JsonValue::Array(array) => {
            write!(out, "{colored_prefix}{braces}")?;
            for (index, value) in array.iter().enumerate() {
                if index != 0 {
                    write!(out, " ")?;
                }
                let new_key = format!("[{index}]");
                display_value_recursive(out, value, &new_key, depth + 1, styler)?;
            }
            write!(out, "{braces_end}")?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_transform_lines_multiple_json() {
        init_logging();
        // Define multiple JSON lines as input
        let input = r#"{"timestamp":1627494000,"level":"info","msg":"Test message 1"}
{"timestamp":1627494001,"level":"error","msg":"Test message 2"}
{"timestamp":1627494002,"level":"debug","msg":"Test message 3"}"#;

        // Expected output after formatting
        let expected = "2021-07-28T17:40:00Z info Test message 1\n\
2021-07-28T17:40:01Z error Test message 2\n\
2021-07-28T17:40:02Z debug Test message 3\n";

        // Use Cursor to simulate I/O streams
        let input_cursor = Cursor::new(input);
        let mut output_cursor = Cursor::new(Vec::new());

        // Set up arguments
        let args = Args {
            no_key_fields: vec![
                "timestamp".to_string(),
                "level".to_string(),
                "msg".to_string(),
            ],
            color: ColorOption::Never, // Disable color for testing simplicity
            timestamp_format: TimestampFormat::Seconds,
            timestamp_field: "timestamp".to_string(),
            level_field: "level".to_string(),
        };

        transform_lines(input_cursor, &mut output_cursor, args);

        let output = String::from_utf8(output_cursor.into_inner()).unwrap();

        assert_eq!(expected, output);
    }

    #[test]
    fn test_transform_lines_with_newlines_in_message() {
        init_logging();
        let input = r#"{"timestamp":1627494000,"level":"info","msg":"Test message with\nnewline"}"#;
        let expected = "2021-07-28T17:40:00Z info\nmsg=\"Test message with\nnewline\"\n";

        let input_cursor = Cursor::new(input);
        let mut output_cursor = Cursor::new(Vec::new());

        let args = Args {
            no_key_fields: vec!["timestamp".to_string(), "level".to_string()],
            color: ColorOption::Never,
            timestamp_format: TimestampFormat::Seconds,
            timestamp_field: "timestamp".to_string(),
            level_field: "level".to_string(),
        };

        transform_lines(input_cursor, &mut output_cursor, args);

        let output = String::from_utf8(output_cursor.into_inner()).unwrap();

        assert_eq!(expected, output);
    }

    #[test]
    fn test_transform_lines_with_nested_objects_no_color() {
        init_logging();
        let input =
            r#"{"timestamp":1627494000,"level":"info","nested":{"key":"value","array":[1,2,3]}}"#;
        let expected = "2021-07-28T17:40:00Z info nested{key=value array{[0]=1 [1]=2 [2]=3}}\n";

        let input_cursor = Cursor::new(input);
        let mut output_cursor = Cursor::new(Vec::new());

        let args = Args {
            no_key_fields: vec!["timestamp".to_string(), "level".to_string()],
            color: ColorOption::Never,
            timestamp_format: TimestampFormat::Seconds,
            timestamp_field: "timestamp".to_string(),
            level_field: "level".to_string(),
        };

        transform_lines(input_cursor, &mut output_cursor, args);

        let output = String::from_utf8(output_cursor.into_inner()).unwrap();
        assert_eq!(expected, output);
    }

    #[test]
    fn test_transform_lines_with_nested_objects_with_color() {
        init_logging();
        let input = r#"{"timestamp":1627494000,"level":"info","nested":{"key":"value"}}"#;

        let input_cursor = Cursor::new(input);
        let mut output_cursor = Cursor::new(Vec::new());

        let args = Args {
            no_key_fields: vec!["timestamp".to_string(), "level".to_string()],
            color: ColorOption::Always,
            timestamp_format: TimestampFormat::Seconds,
            timestamp_field: "timestamp".to_string(),
            level_field: "level".to_string(),
        };

        transform_lines(input_cursor, &mut output_cursor, args);

        let output = String::from_utf8(output_cursor.into_inner()).unwrap();
        let expected = "\u{1b}[2m2021-07-28T17:40:00Z\u{1b}[0m \u{1b}[36minfo\u{1b}[0m \u{1b}[34mnested\u{1b}[0m\u{1b}[34m{\u{1b}[0m\u{1b}[36mkey\u{1b}[0m=value\u{1b}[34m}\u{1b}[0m\n";
        eprintln!("expected: {expected}");
        eprintln!("output  : {output}");
        assert_eq!(expected, output);
    }
}
