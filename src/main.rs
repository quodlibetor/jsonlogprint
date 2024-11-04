use std::io::{self, BufRead, Write};
use serde::{Deserialize, Serialize};
use indexmap::IndexMap;
use clap::Parser;
use colored::*;
use tracing::debug;
use tracing_subscriber::{self, EnvFilter};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Fields to print without logfmt key
    #[arg(short, long, value_delimiter = ',')]
    no_key_fields: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum JsonValue {
    String(String),
    Number(serde_json::Number),
    Bool(bool),
    Null,
    Object(IndexMap<String, JsonValue>),
    Array(Vec<JsonValue>),
    Removed,
}

fn main() {
    let default_filter = std::env::var("JLP_LOG_FILTER").unwrap_or_else(|_| "warn".to_string());
    let env_filter = EnvFilter::new(default_filter);
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .init();

    colored::control::set_override(true);

    let args = Args::parse();
    let no_key_fields = if args.no_key_fields.is_empty() {
        vec!["timestamp".to_string(), "level".to_string(), "msg".to_string()]
    } else {
        args.no_key_fields
    };

    let stdin = io::stdin();
    let handle = stdin.lock();
    let stdout = io::stdout();
    let mut handle_out = stdout.lock();

    for line in handle.lines() {
        match line {
            Ok(json_line) => {
                let deserializer = &mut serde_json::Deserializer::from_str(&json_line);
                match JsonValue::deserialize(deserializer) {
                    Ok(JsonValue::Object(map)) => {
                        if json_to_logfmt(map, &mut handle_out, &no_key_fields).is_some() {
                            writeln!(handle_out).unwrap();
                        } else {
                            writeln!(handle_out, "{}", json_line).unwrap();
                        }
                    }
                    Ok(_) => {
                        debug!("Deserialized value is not an object: {}", json_line);
                        writeln!(handle_out, "{}", json_line).unwrap();
                    }
                    Err(e) => {
                        debug!("Failed to deserialize JSON line: {} with error: {}", json_line, e);
                        writeln!(handle_out, "{}", json_line).unwrap();
                    }
                }
            }
            Err(e) => {
                debug!("Failed to read line from stdin: {}", e);
                writeln!(handle_out).unwrap();
            }
        }
    }
}

fn json_to_logfmt(mut map: IndexMap<String, JsonValue>, handle_out: &mut dyn Write, no_key_fields: &[String]) -> Option<()> {
    let mut newline_fields: Vec<(String, JsonValue)> = Vec::new();

    // Print fields specified in no_key_fields first if they exist
    for key in no_key_fields {
        if let Some(value) = map.get_mut(key) {
            if let JsonValue::String(val_str) = value {
                if val_str.contains('\n') {
                    newline_fields.push((key.clone(), JsonValue::String(val_str.clone())));
                } else {
                    if key == "level" {
                        write!(handle_out, "{} ", colorize_log_level(val_str)).unwrap();
                    } else {
                        write!(handle_out, "{} ", val_str).unwrap();
                    }
                }
            }
            *value = JsonValue::Removed;
        }
    }

    // Print the rest of the fields, excluding Removed variants
    let mut first = true;
    for (key, value) in map {
        if let JsonValue::Removed = value {
            continue;
        }
        if let JsonValue::String(val_str) = &value {
            if val_str.contains('\n') {
                newline_fields.push((key.clone(), value));
                continue;
            }
        }
        if !first {
            write!(handle_out, " ").unwrap();
        }
        write!(handle_out, "{}", value_to_string_recursive(&value, &key, 0, true)).unwrap();
        first = false;
    }

    // Print fields containing newlines at the end
    for (key, value) in newline_fields {
        writeln!(handle_out).unwrap();
        write!(handle_out, "{}", value_to_string_recursive(&value, &key, 0, true)).unwrap();
    }

    Some(())
}

fn value_to_string_recursive(value: &JsonValue, prefix: &str, depth: usize, is_outermost: bool) -> String {
    let colored_prefix = key_color(prefix, depth);
    let dimmed_braces = "{".dimmed();
    let dimmed_braces_end = "}".dimmed();
    match value {
        JsonValue::String(s) => {
            if s.contains(' ') || s.contains('"') || s.contains('\\') {
                format!(r#"{colored_prefix}="{}""#, s.replace('\\', r"\\").replace('"', r#"\""#))
            } else {
                format!("{colored_prefix}={}", s)
            }
        }
        JsonValue::Number(n) => format!("{colored_prefix}={}", n),
        JsonValue::Bool(b) => format!("{colored_prefix}={}", b),
        JsonValue::Null => format!("{colored_prefix}=null"),
        JsonValue::Removed => String::new(), // This won't be used since Removed values are skipped
        JsonValue::Object(map) => {
            let mut parts = Vec::new();
            for (key, value) in map {
                parts.push(value_to_string_recursive(value, key, depth + 1, false));
            }
            if is_outermost {
                format!("{colored_prefix}{dimmed_braces}{}{dimmed_braces_end}", parts.join(" "))
            } else {
                format!("{colored_prefix}{dimmed_braces}{}{}", parts.join(" "), dimmed_braces_end)
            }
        }
        JsonValue::Array(array) => {
            let mut parts = Vec::new();
            for (index, value) in array.iter().enumerate() {
                let new_key = format!("[{index}]");
                parts.push(value_to_string_recursive(value, &new_key, depth + 1, false));
            }
            if is_outermost {
                format!("{colored_prefix}{dimmed_braces} {} {dimmed_braces_end}", parts.join(" "))
            } else {
                format!("{colored_prefix}{dimmed_braces}{}{}", parts.join(" "), dimmed_braces_end)
            }
        }
    }
}

fn key_color(key: &str, depth: usize) -> ColoredString {
    match depth % 3 {
        0 => key.blue(),
        1 => key.cyan(),
        2 => key.green(),
        _ => key.normal(),
    }
}

fn colorize_log_level(level: &str) -> ColoredString {
    match level.to_lowercase().as_str() {
        "crit" | "critical" => level.red().bold(),
        "error" => level.red(),
        "warn" | "warning" => level.yellow(),
        "info" => level.cyan(),
        "debug" => level.blue().dimmed(),
        "trace" => level.dimmed(),
        _ => level.normal(),
    }
}
