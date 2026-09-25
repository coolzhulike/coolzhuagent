mod output;
mod span;
mod trace_id;

use std::env;
use std::fmt::Display;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use output::{ConsoleOutput, FileOutput, GuiCallback, GuiOutput, Output};
use span::SpanClosed;
use trace_id::{SpanId, TraceId};

pub use span::{
    current_context, current_span_id, current_trace_id, enter_span, exit_span, start_span,
    start_span_with_parent, SpanContext, SpanEvent, SpanGuard,
};
pub use trace_id::{SpanId as SpanIdType, TraceId as TraceIdType};

static LOGGER: OnceLock<Logger> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl LogLevel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Self::Error,
            "warn" | "warning" => Self::Warn,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            _ => Self::Info,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp_ms: u128,
    pub level: LogLevel,
    pub app: String,
    pub module: String,
    pub event: String,
    pub message: String,
    pub trace_id: Option<TraceId>,
    pub span_id: Option<SpanId>,
    pub fields: Vec<(String, String)>,
}

struct Logger {
    app: String,
    path: PathBuf,
    min_level: LogLevel,
    file_output: FileOutput,
    console: Option<ConsoleOutput>,
    gui_callback: RwLock<Option<GuiCallback>>,
}

#[must_use]
pub fn log_path() -> Option<&'static Path> {
    LOGGER.get().map(|logger| logger.path.as_path())
}

pub fn init(app: &str) -> io::Result<PathBuf> {
    if let Some(path) = log_path() {
        return Ok(path.to_path_buf());
    }

    let log_dir = env::var("COOLZHU_LOG_DIR")
        .or_else(|_| env::var("CLAW_LOG_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_log_dir());
    let path = log_dir.join(format!("{}.jsonl", sanitize_file_stem(app)));

    let console_enabled = env::var("COOLZHU_LOG_CONSOLE")
        .or_else(|_| env::var("CLAW_LOG_CONSOLE"))
        .map(|v| v.trim().to_ascii_lowercase() == "true" || v == "1")
        .unwrap_or(false);

    let min_level = env::var("COOLZHU_LOG_LEVEL")
        .or_else(|_| env::var("CLAW_LOG_LEVEL"))
        .map(|value| LogLevel::parse(&value))
        .unwrap_or(LogLevel::Info);

    let file_output = FileOutput::new(path.clone())?;
    let logger = Logger {
        app: app.to_string(),
        path: path.clone(),
        min_level,
        file_output,
        console: if console_enabled {
            Some(ConsoleOutput::new(true))
        } else {
            None
        },
        gui_callback: RwLock::new(None),
    };

    let _ = LOGGER.set(logger);
    Ok(path)
}

pub fn set_gui_callback(callback: impl Fn(&LogEntry) + Send + Sync + 'static) {
    if let Some(logger) = LOGGER.get() {
        let cb = Arc::new(callback);
        let _ = logger.gui_callback.write().map(|mut guard| {
            *guard = Some(cb);
        });
    }
}

pub fn error(module: &str, event: &str, message: &str, fields: &[(&str, String)]) {
    emit(LogLevel::Error, module, event, message, fields);
}

#[must_use]
pub fn error_event_fields(error: impl Display, fields: &[(&str, String)]) -> Vec<(String, String)> {
    let mut fields = fields
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect::<Vec<_>>();
    fields.push(("error".to_string(), error.to_string()));
    fields
}

pub fn error_event(
    module: &str,
    event: &str,
    message: &str,
    error: impl Display,
    fields: &[(&str, String)],
) {
    let fields = error_event_fields(error, fields);
    if LOGGER.get().is_none() {
        write_stderr_error_fallback(module, event, message, fields);
        return;
    }

    let borrowed_fields = fields
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect::<Vec<_>>();
    emit(LogLevel::Error, module, event, message, &borrowed_fields);
}

pub fn warn(module: &str, event: &str, message: &str, fields: &[(&str, String)]) {
    emit(LogLevel::Warn, module, event, message, fields);
}

pub fn info(module: &str, event: &str, message: &str, fields: &[(&str, String)]) {
    emit(LogLevel::Info, module, event, message, fields);
}

pub fn debug(module: &str, event: &str, message: &str, fields: &[(&str, String)]) {
    emit(LogLevel::Debug, module, event, message, fields);
}

pub fn trace(module: &str, event: &str, message: &str, fields: &[(&str, String)]) {
    emit(LogLevel::Trace, module, event, message, fields);
}

pub fn emit(level: LogLevel, module: &str, event: &str, message: &str, fields: &[(&str, String)]) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    if level > logger.min_level {
        return;
    }

    let ctx = current_context();
    let trace_id = ctx.as_ref().map(|c| c.trace_id);
    let span_id = ctx.as_ref().map(|c| c.span_id);

    let entry = LogEntry {
        timestamp_ms: unix_millis(),
        level,
        app: logger.app.clone(),
        module: module.to_string(),
        event: event.to_string(),
        message: message.to_string(),
        trace_id,
        span_id,
        fields: fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    };

    logger.file_output.write_event(&entry);

    if let Some(console) = &logger.console {
        console.write_event(&entry);
    }

    if let Ok(guard) = logger.gui_callback.read() {
        if let Some(cb) = guard.as_ref() {
            cb(&entry);
        }
    }
}

pub fn emit_span_closed(span: &SpanClosed) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    if LogLevel::Debug > logger.min_level {
        return;
    }

    logger.file_output.write_span(span);

    if let Some(console) = &logger.console {
        console.write_span(span);
    }

    if let Ok(guard) = logger.gui_callback.read() {
        if let Some(cb) = guard.as_ref() {
            let gui_output = GuiOutput::new(cb.clone());
            gui_output.write_span(span);
        }
    }
}

fn write_stderr_error_fallback(
    module: &str,
    event: &str,
    message: &str,
    fields: Vec<(String, String)>,
) {
    let entry = LogEntry {
        timestamp_ms: unix_millis(),
        level: LogLevel::Error,
        app: "uninitialized".to_string(),
        module: module.to_string(),
        event: event.to_string(),
        message: message.to_string(),
        trace_id: None,
        span_id: None,
        fields,
    };
    eprintln!("{}", output::format_log_entry(&entry));
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            value if value.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", value as u32));
            }
            value => escaped.push(value),
        }
    }
    escaped
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn default_log_dir() -> PathBuf {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join(".coolzhu")
        .join("logs")
}

fn sanitize_file_stem(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.trim_matches('-').is_empty() {
        "coolzhu".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::{escape_json, LogLevel};

    #[test]
    fn parses_log_levels() {
        assert_eq!(LogLevel::parse("error"), LogLevel::Error);
        assert_eq!(LogLevel::parse("warning"), LogLevel::Warn);
        assert_eq!(LogLevel::parse("debug"), LogLevel::Debug);
        assert_eq!(LogLevel::parse("trace"), LogLevel::Trace);
        assert_eq!(LogLevel::parse("unknown"), LogLevel::Info);
    }

    #[test]
    fn escapes_json_strings() {
        assert_eq!(escape_json("a\"b\\c\n"), "a\\\"b\\\\c\\n");
    }

    #[test]
    fn level_ordering() {
        assert!(LogLevel::Error < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Trace);
    }

    #[test]
    fn sanitize_file_stem_removes_special_chars() {
        let result = super::sanitize_file_stem("coolzhu-agent@1.0");
        assert!(result.contains("coolzhu-agent"));
        assert!(!result.contains('@'));
    }

    #[test]
    fn error_event_fields_preserve_context_and_append_error() {
        let error = std::io::Error::new(std::io::ErrorKind::Other, "tray build failed");
        let fields = super::error_event_fields(&error, &[("phase", "setup".to_string())]);

        assert_eq!(
            fields,
            vec![
                ("phase".to_string(), "setup".to_string()),
                ("error".to_string(), "tray build failed".to_string()),
            ]
        );
    }
}
