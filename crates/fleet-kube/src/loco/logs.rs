use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use fleet_core::loco::{AgentLevel, AgentLogLine};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde_json::{Map, Value};

use crate::manager::{ClusterHandle, lock};
use crate::ops::LogRequest;
use crate::task::TaskGuard;

const TIME_KEY: &str = "time";
const LEVEL_KEY: &str = "level";
const MESSAGE_KEY: &str = "msg";
const TIME_FORMAT: &str = "%H:%M:%S";
const ATTRIBUTE_SEPARATOR: &str = "  ";

pub fn agent_line(line: &str, zone: &TimeZone) -> AgentLogLine {
    let Ok(Value::Object(mut record)) = serde_json::from_str::<Value>(line) else {
        return AgentLogLine {
            time: String::new(),
            level: AgentLevel::Info,
            message: line.to_owned(),
        };
    };
    let time = record
        .remove(TIME_KEY)
        .as_ref()
        .and_then(Value::as_str)
        .and_then(|time| time.parse::<Timestamp>().ok())
        .map(|time| {
            time.to_zoned(zone.clone())
                .strftime(TIME_FORMAT)
                .to_string()
        })
        .unwrap_or_default();
    let level = match record.remove(LEVEL_KEY).as_ref().and_then(Value::as_str) {
        Some("ERROR") => AgentLevel::Error,
        Some("WARN") => AgentLevel::Warn,
        _ => AgentLevel::Info,
    };
    let message = record
        .remove(MESSAGE_KEY)
        .as_ref()
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_default();
    AgentLogLine {
        time,
        level,
        message: with_attributes(message, &record),
    }
}

fn with_attributes(message: String, attributes: &Map<String, Value>) -> String {
    let mut sorted: Vec<(&String, &Value)> = attributes.iter().collect();
    sorted.sort_by_key(|(key, _)| *key);
    let mut parts = vec![message];
    parts.extend(sorted.into_iter().map(|(key, value)| match value {
        Value::String(text) => format!("{key}={text}"),
        other => format!("{key}={other}"),
    }));
    parts.join(ATTRIBUTE_SEPARATOR)
}

pub struct Tail {
    lines: Arc<Mutex<VecDeque<String>>>,
    _task: TaskGuard,
}

impl Tail {
    pub fn start(
        handle: &ClusterHandle,
        request: LogRequest,
        limit: usize,
        changed: Arc<AtomicBool>,
    ) -> Self {
        let lines = Arc::new(Mutex::new(VecDeque::new()));
        let buffer = lines.clone();
        let handle = handle.clone();
        let task = TaskGuard::spawn(async move {
            let stream = match handle.logs(&request).await {
                Ok(stream) => stream,
                Err(error) => {
                    tracing::debug!(%error, pod = %request.pod, "loco log stream failed to start");
                    return;
                }
            };
            drop(handle);
            while let Ok(line) = stream.lines.recv().await {
                let Ok(line) = line else { break };
                push_bounded(&mut lock(&buffer), line, limit);
                changed.store(true, Ordering::Relaxed);
            }
        });
        Self { lines, _task: task }
    }

    pub fn lines(&self) -> Vec<String> {
        lock(&self.lines).iter().cloned().collect()
    }
}

pub fn push_bounded(buffer: &mut VecDeque<String>, line: String, limit: usize) {
    buffer.push_back(line);
    while buffer.len() > limit {
        buffer.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_json_lines_read_like_the_design() {
        let line = agent_line(
            r#"{"time":"2026-10-12T21:14:02.123Z","level":"INFO","msg":"placement applied","resource":"checkout","revision":14}"#,
            &TimeZone::UTC,
        );
        assert_eq!(line.time, "21:14:02");
        assert_eq!(line.level, AgentLevel::Info);
        assert_eq!(
            line.message,
            "placement applied  resource=checkout  revision=14"
        );
        let warn = agent_line(r#"{"level":"WARN","msg":"slow"}"#, &TimeZone::UTC);
        assert_eq!((warn.level, warn.time.as_str()), (AgentLevel::Warn, ""));
    }

    #[test]
    fn plain_lines_pass_through() {
        let line = agent_line("panic: boom", &TimeZone::UTC);
        assert_eq!(line.message, "panic: boom");
        assert_eq!(line.level, AgentLevel::Info);
    }

    #[test]
    fn buffers_keep_the_newest_lines() {
        let mut buffer = VecDeque::new();
        for n in 0..5 {
            push_bounded(&mut buffer, n.to_string(), 3);
        }
        assert_eq!(buffer, ["2", "3", "4"]);
    }
}
