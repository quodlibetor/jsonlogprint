use chrono::format::Item;
use chrono::{DateTime, Utc};
use clap::Parser;
use fnv::FnvBuildHasher;
use indexmap::IndexMap;
use serde::de::DeserializeSeed as _;
use std::io::{self, BufRead, BufWriter, Write};
use tracing::{debug, trace, warn};
use tracing_subscriber::{self, EnvFilter};

use deser::JsonValue;

use self::styler::Styler;

mod cfg;
mod deser;
mod styler;

/// The number of seconds between 1970 and 3000
///
/// If timestamp_format = auto we use this to determine if we should convert
/// using millis or seconds.
const YEAR_3K_EPOCH: i64 = 32503698000;

type FnvIndexMap<K, V> = IndexMap<K, V, FnvBuildHasher>;

fn main() {
    let args = cfg::Args::parse();
    let config = cfg::Config::new(args);

    init_logging();
    debug!(config = ?config, "starting up");

    let stdin = io::stdin();
    let handle = stdin.lock();
    let stdout = io::stdout();
    let handle_out = BufWriter::with_capacity(32 * 1024, stdout.lock());

    transform_lines(handle, handle_out, config);
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

struct Reusable<'a> {
    map: FnvIndexMap<&'a str, JsonValue<'a>>,
    newline_fields: Vec<usize>,
}

fn transform_lines(handle: impl BufRead, mut out: impl Write, config: cfg::Config) {
    // Reuse the same map for each line
    let mut reusable = Reusable {
        map: FnvIndexMap::with_capacity_and_hasher(24, FnvBuildHasher::default()),
        newline_fields: Vec::with_capacity(config.no_key_fields.len()),
    };
    let styler = Styler::new(config.color);

    for line in handle.lines() {
        match line {
            Ok(json_line) => {
                process_line(json_line, &mut reusable, &mut out, &config, styler);
                out.flush().unwrap();
            }
            Err(e) => {
                warn!("Failed to read line from stdin: {}", e);
                writeln!(out).unwrap();
            }
        }
    }
}

fn process_line(
    json_line: String,
    reusable: &mut Reusable<'_>,
    out: &mut impl Write,
    config: &cfg::Config,
    styler: Styler,
) {
    if !json_line.starts_with('{') {
        writeln!(out, "{}", json_line).unwrap();
        return;
    }

    // SAFETY: the reusable map contents don't outlive the json_line
    //
    // This function does not return a result, so it's impossible to early exit
    // accidentally with ?, and there are no `return` statements.
    let result = {
        let mut deserializer = unsafe {
            std::mem::transmute::<
                serde_json::Deserializer<serde_json::de::StrRead<'_>>,
                serde_json::Deserializer<serde_json::de::StrRead<'static>>,
            >(serde_json::Deserializer::from_str(&json_line))
        };

        let seed = deser::IndexMapSeed {
            map: &mut reusable.map,
        };
        seed.deserialize(&mut deserializer)
    };

    match result {
        Ok(()) => {
            if let Err(e) = json_to_logfmt(reusable, out, config, styler) {
                debug!("Failed to format JSON line: {}", e);
                writeln!(out).unwrap();
                writeln!(out, "{}", json_line).unwrap();
            }
            writeln!(out).unwrap();
        }
        Err(e) => {
            debug!(
                line = %json_line,
                error = %e,
                "Failed to deserialize JSON line",
            );
            writeln!(out, "{}", json_line).unwrap();
        }
    }
    reusable.map.clear();
    reusable.newline_fields.clear();
}

fn json_to_logfmt(
    storage: &mut Reusable,
    out: &mut impl Write,
    config: &cfg::Config,
    styler: Styler,
) -> io::Result<()> {
    storage.newline_fields.clear();
    let mut first = true;
    // Print fields specified in no_key_fields first if they exist
    for key in &config.no_key_fields {
        if let Some(value) = storage.map.get_mut(key.as_str()) {
            if !first {
                write!(out, " ")?;
            } else {
                first = false;
            }
            match value {
                JsonValue::String(val_str) => {
                    if key == &config.level_field {
                        write!(out, "{}", styler.level(val_str))?;
                    } else {
                        write!(out, "{}", val_str)?;
                    }
                }
                JsonValue::Number(num) => {
                    if key == &config.timestamp_field {
                        let timestamp = num.as_i64().unwrap_or_default();
                        if config.timestamp_format != cfg::TimestampFormat::Raw {
                            try_format_datetime(
                                &config.timestamp_format,
                                timestamp,
                                out,
                                styler,
                                &config.millis_out_format,
                                &config.secs_out_format,
                            )?;
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
    for (index, (key, value)) in storage.map.iter().enumerate() {
        match value {
            JsonValue::Removed => continue,
            JsonValue::String(val_str) if val_str.contains('\n') => {
                storage.newline_fields.push(index);
                continue;
            }
            _ => {
                if !first {
                    write!(out, " ").unwrap();
                }
                display_value_recursive(out, value, key, 0, styler)?;
                first = false;
            }
        }
    }

    // Print fields containing newlines at the end
    for index in &storage.newline_fields {
        writeln!(out).unwrap();
        let (key, value) = storage
            .map
            .get_index(*index)
            .expect("valid indices created");
        display_value_recursive(out, value, key, 0, styler)?;
    }

    Ok(())
}

fn try_format_datetime(
    timestamp_format: &cfg::TimestampFormat,
    timestamp: i64,
    out: &mut impl Write,
    styler: Styler,
    millis_out_format: &[Item],
    secs_out_format: &[Item],
) -> Result<(), io::Error> {
    let mut tsfmt = *timestamp_format;
    let iso_datetime = match timestamp_format {
        cfg::TimestampFormat::Auto if timestamp > YEAR_3K_EPOCH => {
            tsfmt = cfg::TimestampFormat::Millis;
            DateTime::<Utc>::from_timestamp(timestamp / 1000, (timestamp % 1000 * 1_000_000) as u32)
        }
        cfg::TimestampFormat::Auto => {
            tsfmt = cfg::TimestampFormat::Seconds;
            DateTime::<Utc>::from_timestamp(timestamp, 0)
        }
        cfg::TimestampFormat::Seconds => DateTime::<Utc>::from_timestamp(timestamp, 0),
        cfg::TimestampFormat::Millis => {
            DateTime::<Utc>::from_timestamp(timestamp / 1000, (timestamp % 1000 * 1_000_000) as u32)
        }
        cfg::TimestampFormat::Raw => {
            unreachable!("Raw timestamp format should not be used in maybe_format_datetime")
        }
    };

    match (iso_datetime, tsfmt) {
        (Some(dt), cfg::TimestampFormat::Seconds) => {
            write!(
                out,
                "{}",
                styler.timestamp(&dt.format_with_items(secs_out_format.iter()))
            )
            .unwrap();
        }
        (Some(dt), cfg::TimestampFormat::Millis) => {
            write!(
                out,
                "{}",
                styler.timestamp(&dt.format_with_items(millis_out_format.iter()))
            )
            .unwrap();
        }
        _ => {
            write!(out, "{}", styler.timestamp(&timestamp))?;
        }
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
    let (colored_prefix, sep) = if prefix.is_empty() {
        (styler.empty(), "")
    } else {
        (styler.depth(prefix, depth), "=")
    };

    match value {
        JsonValue::String(s) => {
            if s.contains(' ') || s.contains('"') || s.contains('\\') {
                let val = s.replace('\\', r"\\").replace('"', r#"\""#);
                write!(out, r#"{colored_prefix}{sep}"{val}""#)
            } else {
                write!(out, "{colored_prefix}{sep}{s}")
            }
        }
        JsonValue::Number(n) => write!(out, "{colored_prefix}{sep}{n}"),
        JsonValue::Bool(b) => write!(out, "{colored_prefix}{sep}{b}"),
        JsonValue::Null => write!(out, "{colored_prefix}{sep}null"),
        JsonValue::Removed => Ok(()), // This won't be used since Removed values are skipped
        JsonValue::Object(map) => {
            let prefix_braces = styler.depth_multi(prefix, "{", depth);
            write!(out, "{prefix_braces}")?;
            let mut first = true;
            for (key, val) in map.iter() {
                if !first {
                    write!(out, " ")?;
                } else {
                    first = false;
                }
                display_value_recursive(out, val, key, depth + 1, styler)?
            }
            let braces_end = styler.depth("}", depth);
            write!(out, "{braces_end}")?;
            Ok(())
        }
        JsonValue::Array(array) => {
            let braces_start = styler.depth_multi(prefix, "[", depth);
            let mut first = true;
            write!(out, "{braces_start}")?;
            for value in array.iter() {
                if !first {
                    write!(out, " ")?;
                } else {
                    first = false;
                }
                display_value_recursive(out, value, "", depth + 1, styler)?;
            }
            let braces_end = styler.depth("]", depth);
            write!(out, "{braces_end}")?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn test_config() -> cfg::Config {
        cfg::Config {
            no_key_fields: vec![
                "timestamp".to_string(),
                "level".to_string(),
                "msg".to_string(),
            ],
            color: cfg::ColorOption::Never, // Disable color for testing simplicity
            timestamp_format: cfg::TimestampFormat::Seconds,
            timestamp_field: "timestamp".to_string(),
            level_field: "level".to_string(),
            millis_out_format: cfg::default_millis_out_format(),
            secs_out_format: cfg::default_secs_out_format(),
        }
    }

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
        let config = test_config();

        transform_lines(input_cursor, &mut output_cursor, config);

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

        let mut config = test_config();
        config.no_key_fields = vec!["timestamp".to_string(), "level".to_string()];

        transform_lines(input_cursor, &mut output_cursor, config);

        let output = String::from_utf8(output_cursor.into_inner()).unwrap();

        assert_eq!(expected, output);
    }

    #[test]
    fn test_transform_lines_with_nested_objects_no_color() {
        init_logging();
        let input =
            r#"{"timestamp":1627494000,"level":"info","nested":{"key":"value","array":[1,2,3]}}"#;
        let expected = "2021-07-28T17:40:00Z info nested{key=value array[1 2 3]}\n";

        let input_cursor = Cursor::new(input);
        let mut output_cursor = Cursor::new(Vec::new());

        let config = test_config();

        transform_lines(input_cursor, &mut output_cursor, config);

        let output = String::from_utf8(output_cursor.into_inner()).unwrap();
        assert_eq!(expected, output);
    }

    #[test]
    fn test_transform_lines_with_nested_objects_with_color() {
        init_logging();
        let input = r#"{"timestamp":1627494000,"level":"info","nested":{"key":"value"}}"#;

        let input_cursor = Cursor::new(input);
        let mut output_cursor = Cursor::new(Vec::new());

        let mut config = test_config();
        config.color = cfg::ColorOption::Always;

        transform_lines(input_cursor, &mut output_cursor, config);

        let output = String::from_utf8(output_cursor.into_inner()).unwrap();
        let expected = "\u{1b}[2m2021-07-28T17:40:00Z\u{1b}[0m \u{1b}[36minfo\u{1b}[0m \u{1b}[34mnested{\u{1b}[0m\u{1b}[36mkey\u{1b}[0m=value\u{1b}[34m}\u{1b}[0m\n";
        eprint!("expected: {expected}");
        eprint!("output  : {output}");
        assert_eq!(expected, output);
    }

    #[test]
    fn test_transform_lines_non_json_passthrough() {
        init_logging();
        let input = "This is not JSON\nNeither is this line\n{also not json}\n";

        let input_cursor = Cursor::new(input);
        let mut output_cursor = Cursor::new(Vec::new());

        let config = test_config();

        transform_lines(input_cursor, &mut output_cursor, config);

        let output = String::from_utf8(output_cursor.into_inner()).unwrap();
        assert_eq!(input, output);
    }
}
