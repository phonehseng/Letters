//! runs one checker process with a time limit and a way to stop it early.
//!
//! used for compiler syntax checks and for asking a tool its version. the
//! process gets no input, its output is collected on two threads (so a
//! full pipe can never stall it), and it is killed when time runs out or
//! when a newer check makes this one pointless.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// how often a running process is looked at. short enough that a stopped
/// check ends within a blink, long enough to cost nothing
const POLL_INTERVAL: Duration = Duration::from_millis(15);

/// environment variables that make a tool load extra code on start (a java
/// agent, a node preload script). a syntax check must never run anything
/// but the checker itself, so they are removed for every run
const CODE_LOADING_VARIABLES: [&str; 6] = [
    "NODE_OPTIONS",
    "JAVA_TOOL_OPTIONS",
    "JDK_JAVA_OPTIONS",
    "_JAVA_OPTIONS",
    "PYTHONSTARTUP",
    "PYTHONPATH",
];

/// what a finished (or stopped) process said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub cancelled: bool,
}

/// a ticket for "the newest check". starting a check takes a new number;
/// a running check whose number is no longer the newest stops itself.
#[derive(Clone, Default)]
pub struct CheckGeneration {
    newest: Arc<AtomicU64>,
}

impl CheckGeneration {
    pub fn start_new(&self) -> u64 {
        self.newest.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// stops every running check without starting another.
    pub fn cancel_all(&self) {
        self.newest.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_current(&self, ticket: u64) -> bool {
        self.newest.load(Ordering::SeqCst) == ticket
    }
}

/// runs `command` until it ends, `timeout` passes, or `still_wanted` says no.
pub fn run_with_limits(
    mut command: Command,
    timeout: Duration,
    still_wanted: &dyn Fn() -> bool,
) -> std::io::Result<RunOutput> {
    for name in CODE_LOADING_VARIABLES {
        command.env_remove(name);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console_window(&mut command);
    let mut child = command.spawn()?;
    let stdout_reader = collect(child.stdout.take());
    let stderr_reader = collect(child.stderr.take());
    let (exit_code, timed_out, cancelled) = wait_within(&mut child, timeout, still_wanted)?;
    Ok(RunOutput {
        exit_code,
        stdout: stdout_reader.join().unwrap_or_default(),
        stderr: stderr_reader.join().unwrap_or_default(),
        timed_out,
        cancelled,
    })
}

/// waits for the child, killing it when it runs too long or isn't wanted.
fn wait_within(
    child: &mut Child,
    timeout: Duration,
    still_wanted: &dyn Fn() -> bool,
) -> std::io::Result<(Option<i32>, bool, bool)> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status.code(), false, false));
        }
        let timed_out = started.elapsed() >= timeout;
        let cancelled = !still_wanted();
        if timed_out || cancelled {
            let _ = child.kill();
            let _ = child.wait();
            return Ok((None, timed_out, cancelled));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn collect(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

// on windows a console program started from a window app flashes a black
// console window unless it is told not to make one
#[cfg(windows)]
pub(crate) fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub(crate) fn hide_console_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    // a command that runs for a long time on every system, without a shell script
    fn slow_command() -> Command {
        if cfg!(windows) {
            let mut command = Command::new("ping");
            command.args(["-n", "30", "127.0.0.1"]);
            command
        } else {
            let mut command = Command::new("sleep");
            command.arg("30");
            command
        }
    }

    #[test]
    fn a_process_that_runs_too_long_is_stopped() {
        let started = Instant::now();
        let output = run_with_limits(slow_command(), Duration::from_millis(300), &|| true).unwrap();
        assert!(output.timed_out);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_check_that_is_no_longer_newest_stops_itself() {
        let generation = CheckGeneration::default();
        let ticket = generation.start_new();
        generation.start_new();
        let output = run_with_limits(slow_command(), Duration::from_secs(20), &|| {
            generation.is_current(ticket)
        })
        .unwrap();
        assert!(output.cancelled);
        assert!(!output.timed_out);
    }

    #[test]
    fn a_missing_program_is_an_error_not_a_hang() {
        let command = Command::new("letters-no-such-program-anywhere");
        assert!(run_with_limits(command, Duration::from_secs(1), &|| true).is_err());
    }
}
