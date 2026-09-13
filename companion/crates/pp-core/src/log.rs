//! A small bounded log the UI can render. Errors are surfaced, never swallowed.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn label(&self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub at: String,
    pub level: Level,
    pub text: String,
}

#[derive(Debug)]
pub struct Log {
    lines: VecDeque<LogLine>,
    cap: usize,
    /// Total ever recorded, so the UI can tell that older lines were dropped.
    pub total: u64,
}

impl Default for Log {
    fn default() -> Self {
        Self::with_capacity(500)
    }
}

impl Log {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(cap.min(1024)),
            cap,
            total: 0,
        }
    }

    pub fn push(&mut self, level: Level, text: impl Into<String>) {
        if self.lines.len() == self.cap {
            self.lines.pop_front();
        }
        self.lines.push_back(LogLine {
            at: crate::now_hms(),
            level,
            text: text.into(),
        });
        self.total += 1;
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &LogLine> {
        self.lines.iter()
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oldest_lines_are_dropped_once_full() {
        let mut log = Log::with_capacity(3);
        for i in 0..5 {
            log.push(Level::Info, format!("line {i}"));
        }
        assert_eq!(log.len(), 3);
        assert_eq!(log.total, 5);
        let texts: Vec<&str> = log.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["line 2", "line 3", "line 4"]);
    }
}
