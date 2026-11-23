use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
        }
    }

    fn parse(s: &str) -> Option<Option<Level>> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "silent" => Some(None),
            "error" => Some(Some(Level::Error)),
            "warn" | "warning" => Some(Some(Level::Warn)),
            "info" => Some(Some(Level::Info)),
            "debug" | "trace" => Some(Some(Level::Debug)),
            _ => None,
        }
    }
}

const OFF: u8 = u8::MAX;
static LEVEL: AtomicU8 = AtomicU8::new(OFF);
static INIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn current_level() -> Option<Level> {
    if !INIT.swap(true, Ordering::Relaxed) {
        let parsed = std::env::var("ZER0_LOG")
            .ok()
            .as_deref()
            .and_then(Level::parse)
            .unwrap_or(Some(Level::Warn));
        LEVEL.store(parsed.map_or(OFF, |l| l as u8), Ordering::Relaxed);
    }
    match LEVEL.load(Ordering::Relaxed) {
        OFF => None,
        0 => Some(Level::Error),
        1 => Some(Level::Warn),
        2 => Some(Level::Info),
        _ => Some(Level::Debug),
    }
}

pub fn set_level(level: Option<Level>) {
    LEVEL.store(level.map_or(OFF, |l| l as u8), Ordering::Relaxed);
    INIT.store(true, Ordering::Relaxed);
}

pub fn format_line(level: Level, event: &str, fields: &serde_json::Value) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut v = serde_json::json!({
        "ts": ts,
        "level": level.as_str(),
        "event": event,
    });
    if let (Some(obj), Some(extra)) = (v.as_object_mut(), fields.as_object()) {
        for (k, val) in extra {
            if k != "ts" && k != "level" && k != "event" {
                obj.insert(k.clone(), val.clone());
            }
        }
    }
    v.to_string()
}

pub fn log(level: Level, event: &str, fields: serde_json::Value) {
    let Some(max) = current_level() else {
        return;
    };
    if level > max {
        return;
    }
    eprintln!("{}", format_line(level, event, &fields));
}

pub fn log_error(event: &str, fields: serde_json::Value) {
    log(Level::Error, event, fields);
}

pub fn log_warn(event: &str, fields: serde_json::Value) {
    log(Level::Warn, event, fields);
}

pub fn log_info(event: &str, fields: serde_json::Value) {
    log(Level::Info, event, fields);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn format_line_is_valid_json_with_reserved_keys() {
        let line = format_line(Level::Warn, "telegram_poll", &json!({ "error": "timeout" }));
        let v: serde_json::Value = serde_json::from_str(&line).expect("JSON invalide");
        assert_eq!(v["level"], "warn");
        assert_eq!(v["event"], "telegram_poll");
        assert_eq!(v["error"], "timeout");
        assert!(v["ts"].is_u64());
    }

    #[test]
    fn format_line_refuses_to_override_reserved_keys() {
        let line = format_line(
            Level::Error,
            "audit_write",
            &json!({ "level": "debug", "event": "fake", "ts": 0, "error": "disk full" }),
        );
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["level"], "error", "niveau usurpé : {line}");
        assert_eq!(v["event"], "audit_write", "événement usurpé : {line}");
        assert_ne!(v["ts"], 0, "timestamp usurpé : {line}");
        assert_eq!(v["error"], "disk full");
    }

    #[test]
    fn level_parsing_covers_off_and_unknown() {
        assert_eq!(Level::parse("off"), Some(None));
        assert_eq!(Level::parse("WARN"), Some(Some(Level::Warn)));
        assert_eq!(Level::parse(" debug "), Some(Some(Level::Debug)));
        assert_eq!(Level::parse("bavard"), None);
    }

    #[test]
    fn levels_are_ordered_from_error_to_debug() {
        assert!(Level::Error < Level::Warn);
        assert!(Level::Warn < Level::Info);
        assert!(Level::Info < Level::Debug);
    }

    #[test]
    fn filter_drops_events_below_the_threshold() {
        set_level(Some(Level::Warn));
        assert!(Level::Error <= current_level().unwrap());
        assert!(Level::Warn <= current_level().unwrap());
        assert!(Level::Info > current_level().unwrap());
        assert!(Level::Debug > current_level().unwrap());

        set_level(None);
        assert!(current_level().is_none(), "off doit tout couper");

        set_level(Some(Level::Warn));
    }
}
