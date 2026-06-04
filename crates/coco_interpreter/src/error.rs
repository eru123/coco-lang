/// Control flow signals and runtime errors for the interpreter.

#[derive(Debug, Clone)]
pub enum ControlFlow {
    Return(super::value::Value),
    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
}

impl RuntimeError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RuntimeError: {}", self.message)
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
