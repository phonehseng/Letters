//! running a converter program with a time limit, so a program waiting
//! on a hidden dialog can't hang a conversion forever.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use letters_ocr::CancelFlag;

/// how a run ended.
pub(crate) enum RunEnd {
    Finished {
        status: ExitStatus,
        lines: Vec<String>,
        errors: String,
    },
    TimedOut,
    Cancelled,
    /// the program couldn't be started at all
    NotStarted(String),
}

/// runs `command`, reading what it prints line by line. `extra_process`
/// is handed each line and may name another process the program started
/// (an office program it drives); that process is stopped too if the run
/// times out or is cancelled.
pub(crate) fn run_with_limit(
    mut command: Command,
    limit: Duration,
    cancel: &CancelFlag,
    extra_process: impl Fn(&str) -> Option<u32>,
) -> RunEnd {
    hide_window(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return RunEnd::NotStarted(error.to_string()),
    };
    let lines = read_lines(&mut child);
    let errors = read_all(&mut child);
    let started = Instant::now();
    let mut seen = Vec::new();
    let mut helper: Option<u32> = None;
    loop {
        while let Ok(line) = lines.try_recv() {
            if let Some(pid) = extra_process(&line) {
                helper = Some(pid);
            }
            seen.push(line);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // the last lines may still be on their way (but a program
                // it started may keep the output open, so don't wait long)
                while let Ok(line) = lines.recv_timeout(Duration::from_millis(300)) {
                    seen.push(line);
                }
                let errors = errors
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap_or_default();
                return RunEnd::Finished {
                    status,
                    lines: seen,
                    errors,
                };
            }
            Ok(None) => {}
            Err(error) => return RunEnd::NotStarted(error.to_string()),
        }
        let stop = if cancel.is_cancelled() {
            Some(RunEnd::Cancelled)
        } else if started.elapsed() > limit {
            Some(RunEnd::TimedOut)
        } else {
            None
        };
        if let Some(end) = stop {
            stop_process_tree(&mut child, helper);
            return end;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn read_lines(child: &mut Child) -> mpsc::Receiver<String> {
    let (send, receive) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if send.send(line.trim().to_string()).is_err() {
                    break;
                }
            }
        });
    }
    receive
}

fn read_all(child: &mut Child) -> mpsc::Receiver<String> {
    let (send, receive) = mpsc::channel();
    if let Some(mut stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            let _ = send.send(text);
        });
    }
    receive
}

/// stops the program, everything it started, and the helper process it
/// named (which, being started through com, isn't its child).
fn stop_process_tree(child: &mut Child, helper: Option<u32>) {
    #[cfg(windows)]
    {
        let mut ids = vec![child.id()];
        ids.extend(helper);
        for id in ids {
            let mut kill = Command::new("taskkill");
            kill.args(["/PID", &id.to_string(), "/T", "/F"]);
            hide_window(&mut kill);
            let _ = kill.output();
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(id) = helper {
            let _ = Command::new("kill").args(["-9", &id.to_string()]).output();
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// keeps a console window from flashing up on windows.
pub(crate) fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn slow_command() -> Command {
        let mut command = Command::new("powershell");
        command.args([
            "-NoProfile",
            "-Command",
            "Write-Output 'ours=1'; Start-Sleep -Seconds 30",
        ]);
        command
    }

    #[cfg(windows)]
    #[test]
    fn slow_programs_are_stopped() {
        let started = Instant::now();
        let end = run_with_limit(
            slow_command(),
            Duration::from_secs(3),
            &CancelFlag::new(),
            |_| None,
        );
        assert!(matches!(end, RunEnd::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(15));
    }

    #[cfg(windows)]
    #[test]
    fn cancelling_stops_the_program() {
        let cancel = CancelFlag::new();
        cancel.cancel();
        let end = run_with_limit(slow_command(), Duration::from_secs(60), &cancel, |_| None);
        assert!(matches!(end, RunEnd::Cancelled));
    }

    #[test]
    fn missing_programs_are_reported() {
        let end = run_with_limit(
            Command::new("letters-no-such-program"),
            Duration::from_secs(5),
            &CancelFlag::new(),
            |_| None,
        );
        assert!(matches!(end, RunEnd::NotStarted(_)));
    }

    #[cfg(windows)]
    #[test]
    fn printed_lines_are_collected() {
        let mut command = Command::new("cmd");
        command.args(["/C", "echo one&& echo two"]);
        match run_with_limit(command, Duration::from_secs(20), &CancelFlag::new(), |_| {
            None
        }) {
            RunEnd::Finished { status, lines, .. } => {
                assert!(status.success());
                assert_eq!(lines, ["one", "two"]);
            }
            _ => panic!("the command should finish"),
        }
    }
}
