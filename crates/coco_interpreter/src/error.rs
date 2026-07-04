/// Control flow signals and runtime errors for the interpreter.
use std::path::PathBuf;

use coco_span::Span;

use crate::stack::CallStack;

#[derive(Debug, Clone)]
pub enum ControlFlow {
    Return(super::value::Value),
    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
    /// The source span where the error occurred (if known).
    pub span: Option<Span>,
    /// The source file path (if known).
    pub file: Option<PathBuf>,
    /// Snapshot of the call stack at error time.
    pub stack_trace: Vec<crate::stack::StackFrame>,
}

impl RuntimeError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            span: None,
            file: None,
            stack_trace: Vec::new(),
        }
    }

    /// Attach a source span to the error.
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// Attach a source file path.
    pub fn with_file(mut self, file: PathBuf) -> Self {
        self.file = Some(file);
        self
    }

    /// Attach a call stack snapshot.
    pub fn with_stack(mut self, stack: &CallStack) -> Self {
        self.stack_trace = stack.snapshot();
        self
    }

    /// Format the error with location and optional stack trace.
    pub fn format(&self, verbose: bool) -> String {
        let location = match (&self.file, &self.span) {
            (Some(file), Some(span)) => {
                format!("{}:{}:{}", file.display(), span.start, span.end)
            }
            (Some(file), None) => format!("{}", file.display()),
            _ => String::from("<unknown>"),
        };

        let mut msg = format!("{}: {}", location, self.message);

        // Show stack trace
        if !self.stack_trace.is_empty() {
            let frames: Vec<&crate::stack::StackFrame> = if verbose {
                self.stack_trace.iter().rev().collect()
            } else {
                // Show only the top frame
                vec![&self.stack_trace[self.stack_trace.len() - 1]]
            };

            if self.stack_trace.len() > 1 && verbose {
                msg.push_str(&format!(
                    "\n  ({} frames in stack trace, use --verbose for all)",
                    self.stack_trace.len()
                ));
            }

            for (i, frame) in frames.iter().enumerate() {
                let loc = if let (Some(f), Some(s)) = (&frame.file, &frame.call_site) {
                    format!("{}:{}:{}", f.display(), s.start, s.end)
                } else {
                    String::from("<unknown>")
                };
                if verbose && self.stack_trace.len() > 1 {
                    msg.push_str(&format!(
                        "\n  #{} {} at {}",
                        self.stack_trace.len() - i,
                        frame.function_name,
                        loc
                    ));
                } else {
                    msg.push_str(&format!("\n  at {} ({})", frame.function_name, loc));
                }
            }
        }

        msg
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format(false))
    }
}

impl std::error::Error for RuntimeError {}

/// Result type used throughout the interpreter.
/// Ok(Value) for normal evaluation, Err for both errors and control flow.
pub type IResult = Result<super::value::Value, Signal>;

/// Signal combines runtime errors and control flow into one Err type.
#[derive(Debug, Clone)]
pub enum Signal {
    Error(RuntimeError),
    Flow(ControlFlow),
}

impl From<RuntimeError> for Signal {
    fn from(e: RuntimeError) -> Self {
        Signal::Error(e)
    }
}

impl From<ControlFlow> for Signal {
    fn from(cf: ControlFlow) -> Self {
        Signal::Flow(cf)
    }
}
