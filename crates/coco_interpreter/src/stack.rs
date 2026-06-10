/// Call stack tracking for error reporting.

use std::path::PathBuf;

use coco_span::Span;

/// A single call stack frame.
#[derive(Debug, Clone)]
pub struct StackFrame {
    /// Name of the function being called.
    pub function_name: String,
    /// Span of the function definition (where it's declared).
    pub def_span: Option<Span>,
    /// Span of the call site (where it was invoked).
    pub call_site: Option<Span>,
    /// Source file path for this frame.
    pub file: Option<PathBuf>,
}

/// A call stack — tracks function entry/exit during execution.
#[derive(Debug, Clone)]
pub struct CallStack {
    frames: Vec<StackFrame>,
}

impl CallStack {
    /// Create an empty call stack.
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    /// Push a new frame onto the stack.
    pub fn push(&mut self, frame: StackFrame) {
        self.frames.push(frame);
    }

    /// Pop the top frame.
    pub fn pop(&mut self) {
        self.frames.pop();
    }

    /// Get the most recent frame (top of stack).
    pub fn top(&self) -> Option<&StackFrame> {
        self.frames.last()
    }

    /// Number of frames in the stack.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Get a snapshot of the current stack (bottom to top).
    pub fn snapshot(&self) -> Vec<StackFrame> {
        self.frames.clone()
    }

    /// Format the stack trace. If `verbose` is true, show all frames;
    /// otherwise show only the top frame (most recent call).
    pub fn format_trace(&self, verbose: bool) -> String {
        if self.frames.is_empty() {
            return String::new();
        }

        let frames: Vec<&StackFrame> = if verbose {
            self.frames.iter().rev().collect()
        } else {
            // Default: show only the top frame
            vec![&self.frames[self.frames.len() - 1]]
        };

        let mut result = String::from("\nStack trace:\n");
        for (i, frame) in frames.iter().enumerate() {
            let location = if let (Some(file), Some(span)) = (&frame.file, &frame.call_site) {
                format!("{}:{}:{}", file.display(), span.start, span.end)
            } else if let Some(file) = &frame.file {
                format!("{}", file.display())
            } else {
                String::from("<unknown>")
            };
            if verbose && self.frames.len() > 1 {
                result.push_str(&format!(
                    "  #{} {} at {}\n",
                    self.frames.len() - i,
                    frame.function_name,
                    location
                ));
            } else {
                result.push_str(&format!(
                    "  at {} ({})\n",
                    frame.function_name,
                    location
                ));
            }
        }
        result
    }
}

impl Default for CallStack {
    fn default() -> Self {
        Self::new()
    }
}
