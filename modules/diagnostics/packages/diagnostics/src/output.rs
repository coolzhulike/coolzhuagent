use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::span::SpanClosed;
use crate::{escape_json, unix_millis, LogEntry, LogLevel};

pub trait Output: Send + Sync {
    fn write_event(&self, entry: &LogEntry);
    fn write_span(&self, span: &SpanClosed);
}

#[derive(Debug)]
pub struct FileOutput {
    file: Mutex<File>,
    #[allow(dead_code)]
    path: PathBuf,
}

impl FileOutput {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(path.parent().unwrap_or(&PathBuf::from(".")))?;
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            file: Mutex::new(file),
            path,
        })
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Output for FileOutput {
    fn write_event(&self, entry: &LogEntry) {
        if let Ok(mut file) = self.file.lock() {
            let line = format_log_entry(entry);
            let _ = writeln!(file, "{}", line);
        }
    }

    fn write_span(&self, span: &SpanClosed) {
        if let Ok(mut file) = self.file.lock() {
            let line = format_span_closed(span);
            let _ = writeln!(file, "{}", line);
        }
    }
}

pub(crate) fn format_log_entry(entry: &LogEntry) -> String {
    let mut line = format!(
        "{{\"ts_ms\":{},\"level\":\"{}\",\"app\":\"{}\",\"module\":\"{}\",\"event\":\"{}\",\"message\":\"{}\"",
        entry.timestamp_ms,
        entry.level.as_str(),
        escape_json(&entry.app),
        escape_json(&entry.module),
        escape_json(&entry.event),
        escape_json(&entry.message)
    );

    if let Some(trace_id) = &entry.trace_id {
        line.push_str(&format!(",\"trace_id\":\"{}\"", trace_id.to_hex()));
    }
    if let Some(span_id) = &entry.span_id {
        line.push_str(&format!(",\"span_id\":\"{}\"", span_id.to_hex()));
    }

    if !entry.fields.is_empty() {
        line.push_str(",\"fields\":{");
        for (index, (key, value)) in entry.fields.iter().enumerate() {
            if index > 0 {
                line.push(',');
            }
            line.push('"');
            line.push_str(&escape_json(key));
            line.push_str("\":\"");
            line.push_str(&escape_json(value));
            line.push('"');
        }
        line.push('}');
    }

    line.push('}');
    line
}

fn format_span_closed(span: &SpanClosed) -> String {
    let mut line = format!(
        "{{\"ts_ms\":{},\"type\":\"span\",\"trace_id\":\"{}\",\"span_id\":\"{}\",\"name\":\"{}\",\"module\":\"{}\",\"duration_ms\":{}",
        unix_millis(),
        span.trace_id.to_hex(),
        span.span_id.to_hex(),
        escape_json(&span.name),
        escape_json(&span.module),
        span.duration_ms
    );

    if let Some(parent_id) = span.parent_id {
        line.push_str(&format!(",\"parent_id\":\"{}\"", parent_id.to_hex()));
    }

    if !span.attributes.is_empty() {
        line.push_str(",\"attributes\":{");
        let attrs: Vec<String> = span
            .attributes
            .iter()
            .map(|(k, v)| format!("\"{}\":\"{}\"", escape_json(k), escape_json(v)))
            .collect();
        line.push_str(&attrs.join(","));
        line.push('}');
    }

    line.push('}');
    line
}

#[derive(Debug)]
pub struct ConsoleOutput {
    ansi: bool,
}

impl ConsoleOutput {
    #[must_use]
    pub const fn new(ansi: bool) -> Self {
        Self { ansi }
    }
}

impl Output for ConsoleOutput {
    fn write_event(&self, entry: &LogEntry) {
        let level_color = if self.ansi {
            match entry.level {
                LogLevel::Error => "\x1b[31m",
                LogLevel::Warn => "\x1b[33m",
                LogLevel::Info => "\x1b[32m",
                LogLevel::Debug => "\x1b[36m",
                LogLevel::Trace => "\x1b[90m",
            }
        } else {
            ""
        };

        let reset = if self.ansi { "\x1b[0m" } else { "" };

        let trace_info = if let Some(trace_id) = &entry.trace_id {
            format!(
                "[{}]",
                trace_id.to_hex().chars().take(8).collect::<String>()
            )
        } else {
            String::new()
        };

        eprintln!(
            "{}{}{} {}{} {}: {}",
            level_color,
            entry.level.as_str(),
            reset,
            trace_info,
            entry.module,
            entry.event,
            entry.message
        );
    }

    fn write_span(&self, span: &SpanClosed) {
        let color = if self.ansi { "\x1b[35m" } else { "" };
        let reset = if self.ansi { "\x1b[0m" } else { "" };

        eprintln!(
            "{}SPAN{} {} {} {} {}ms",
            color,
            reset,
            span.name,
            span.module,
            span.trace_id.to_hex().chars().take(8).collect::<String>(),
            span.duration_ms
        );
    }
}

pub type GuiCallback = Arc<dyn Fn(&LogEntry) + Send + Sync>;

pub struct GuiOutput {
    callback: GuiCallback,
}

impl GuiOutput {
    pub fn new(callback: GuiCallback) -> Self {
        Self { callback }
    }
}

impl Output for GuiOutput {
    fn write_event(&self, entry: &LogEntry) {
        (self.callback)(entry);
    }

    fn write_span(&self, span: &SpanClosed) {
        let entry = LogEntry {
            timestamp_ms: unix_millis() as u128,
            level: LogLevel::Debug,
            app: String::new(),
            module: span.module.clone(),
            event: format!("span_closed:{}", span.name),
            message: format!("duration={}ms", span.duration_ms),
            trace_id: Some(span.trace_id),
            span_id: Some(span.span_id),
            fields: span
                .attributes
                .iter()
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect(),
        };
        (self.callback)(&entry);
    }
}

#[cfg(test)]
mod tests {
    use crate::trace_id::TraceId;

    use super::*;

    #[test]
    fn file_output_writes_jsonl() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_output.jsonl");
        let output = FileOutput::new(path.clone()).expect("create file");

        let entry = LogEntry {
            timestamp_ms: 1000,
            level: LogLevel::Info,
            app: "test_app".to_string(),
            module: "test_module".to_string(),
            event: "test_event".to_string(),
            message: "test message".to_string(),
            trace_id: None,
            span_id: None,
            fields: vec![("key".to_string(), "value".to_string())],
        };

        output.write_event(&entry);

        let content = std::fs::read_to_string(&path).expect("read file");
        assert!(content.contains("\"level\":\"INFO\""));
        assert!(content.contains("\"module\":\"test_module\""));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn console_output_formats_level() {
        let output = ConsoleOutput::new(false);
        let entry = LogEntry {
            timestamp_ms: 1000,
            level: LogLevel::Error,
            app: "app".to_string(),
            module: "mod".to_string(),
            event: "evt".to_string(),
            message: "msg".to_string(),
            trace_id: None,
            span_id: None,
            fields: vec![],
        };
        output.write_event(&entry);
    }

    #[test]
    fn format_log_entry_with_trace_id() {
        let trace_id = TraceId::generate();
        let entry = LogEntry {
            timestamp_ms: 1000,
            level: LogLevel::Debug,
            app: "app".to_string(),
            module: "mod".to_string(),
            event: "evt".to_string(),
            message: "msg".to_string(),
            trace_id: Some(trace_id),
            span_id: None,
            fields: vec![],
        };
        let formatted = format_log_entry(&entry);
        assert!(formatted.contains("trace_id"));
    }
}
