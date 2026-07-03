//! Async I/O event loop integration via `mio`.
//!
//! Provides `io_wait(handle, want_read, [timeout_ms])` — an efficient,
//! epoll/kqueue-backed poll that blocks the OS thread only until the given
//! TCP handle's fd is ready for reading or writing (or the timeout elapses).
//! This lets the HTTP server and other I/O code avoid busy-waiting and
//! compose with the cooperative scheduler: a task can `io_wait` before a
//! blocking `tcp_read`/`tcp_accept`, yielding CPU until data is available.
//!
//! Full task-suspension integration (suspending the Coco task and waking it
//! from the mio event loop) is a larger refactor; this module provides the
//! fd-readiness primitive that enables non-blocking I/O patterns today.

use std::os::fd::AsRawFd;
use std::time::Duration;

use mio::{Events, Interest, Poll, Token, unix::SourceFd};

use crate::builtins::{with_tcp_listener, with_tcp_stream};
use crate::error::{RuntimeError, Signal};
use crate::value::Value;

/// Extract the raw fd for a TCP handle (listener or stream), or None if the
/// handle isn't a known TCP resource. Uses the registry helpers to safely
/// borrow the socket; returns just the fd (an i32, Copy).
fn fd_of(handle: usize) -> Option<i32> {
    let mut fd: Option<i32> = None;
    // Try as a listener first.
    let _ = with_tcp_listener(handle, |listener| {
        fd = Some(listener.as_raw_fd());
        Ok(Value::Null)
    });
    if fd.is_some() {
        return fd;
    }
    // Then as a stream.
    let _ = with_tcp_stream(handle, |stream| {
        fd = Some(stream.as_raw_fd());
        Ok(Value::Null)
    });
    fd
}

/// `io_wait(handle, want_read, [timeout_ms]) -> bool`
///
/// Polls the fd behind `handle` (a TCP listener or stream) for readability
/// (if `want_read` is true) or writability, blocking at most `timeout_ms`
/// (0 = non-blocking, returns immediately). Returns true if ready, false on
/// timeout or if the handle is not a recognized TCP resource.
pub fn io_wait(args: &[Value]) -> Result<Value, Signal> {
    if args.len() < 2 || args.len() > 3 {
        return Err(Signal::Error(RuntimeError::new(
            "io_wait(handle, want_read, [timeout_ms]) expects 2 or 3 arguments",
        )));
    }
    use num_traits::ToPrimitive;
    let handle = match &args[0] {
        Value::Int(n) => n.to_usize().unwrap_or(0), Value::Int64(n) => (*n as usize),
        _ => return Err(Signal::Error(RuntimeError::new("io_wait: handle must be int"))),
    };
    let want_read = match &args[1] {
        Value::Bool(b) => *b,
        _ => return Err(Signal::Error(RuntimeError::new("io_wait: want_read must be bool"))),
    };
    let timeout_ms = match args.get(2) {
        Some(Value::Int(n)) => n.to_u64().unwrap_or(0),
        Some(Value::Int64(n)) => *n as u64,
        Some(Value::Null) | None => 0,
        _ => 0,
    };

    let fd = match fd_of(handle) {
        Some(fd) => fd,
        None => return Ok(Value::Bool(false)),
    };

    let mut poll = Poll::new().map_err(|e| {
        Signal::Error(RuntimeError::new(format!("io_wait: poll create failed: {}", e)))
    })?;
    let mut events = Events::with_capacity(1);
    let token = Token(handle);
    let interest = if want_read {
        Interest::READABLE
    } else {
        Interest::WRITABLE
    };
    let mut source = SourceFd(&fd);
    poll.registry().register(&mut source, token, interest).map_err(|e| {
        Signal::Error(RuntimeError::new(format!("io_wait: register failed: {}", e)))
    })?;

    let timeout = if timeout_ms == 0 {
        // 0 means non-blocking: return immediately if not ready.
        Some(Duration::ZERO)
    } else {
        Some(Duration::from_millis(timeout_ms))
    };
    poll.poll(&mut events, timeout).map_err(|e| {
        Signal::Error(RuntimeError::new(format!("io_wait: poll failed: {}", e)))
    })?;
    let _ = poll.registry().deregister(&mut source);
    Ok(Value::Bool(!events.is_empty()))
}
