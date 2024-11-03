use chrono::Utc;
use rand::Rng;
use serde_json::json;
use std::io::Write;

const MESSAGES: &[&str] = &[
    "Application started",
    "Processing request",
    "Database query executed",
    "Cache miss",
    "Cache hit",
    "Request completed",
    "Connection established",
    "Authentication successful",
    "File processed",
    "Task completed",
];

const LEVELS: &[&str] = &["INFO", "WARN", "ERROR", "DEBUG"];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let count = args
        .get(1)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1000);

    let mut rng = rand::thread_rng();
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    for _ in 0..count {
        let message = MESSAGES[rng.gen_range(0..MESSAGES.len())];
        let level = LEVELS[rng.gen_range(0..LEVELS.len())];

        // Occasionally generate non-JSON lines
        if rng.gen_ratio(1, 20) {
            // 5% chance
            writeln!(handle, "Plain text log message: {}", message).unwrap();
            continue;
        }

        // Sometimes add extra fields
        let mut log = json!({
            "timestamp": Utc::now().timestamp_millis(),
            "level": level,
            "message": message,
            "request_id": format!("req-{}", rng.gen_range(1000..9999)),
        });

        if rng.gen_ratio(1, 2) {
            // 50% chance
            log.as_object_mut()
                .unwrap()
                .insert("duration_ms".to_string(), json!(rng.gen_range(1..1000)));
        }

        if rng.gen_ratio(1, 3) {
            // 33% chance
            log.as_object_mut().unwrap().insert(
                "user_id".to_string(),
                json!(format!("user-{}", rng.gen_range(1..100))),
            );
        }

        writeln!(handle, "{}", serde_json::to_string(&log).unwrap()).unwrap();
    }
}
