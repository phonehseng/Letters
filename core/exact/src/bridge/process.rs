//! running the export script with a time limit, so a word copy waiting on
//! a hidden dialog can't hang a conversion forever. adapted from
//! `core/convert`'s office runner.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::CancelFlag;

/// how a run ended.
pub(crate) enum RunEnd {
    Finished {
        status: ExitStatus,
        lines: Vec<String>,
        errors: String,
    },
    TimedOut {
        lines: Vec<String>,
    },
    Cancelled {
        lines: Vec<String>,
    },
    /// the program couldn't be started at all
    NotStarted(String),
}

/// runs `command`, reading what it prints line by line. `helper` is handed
/// each line and may name the word process the script started; that process
/// is stopped too if the run times out or is cancelled.
pub(crate) fn run_with_limit(
    mut command: Command,
    limit: Duration,
    cancel: &CancelFlag,
    helper: impl Fn(&str) -> Option<u32>,
    helper_grace: Duration,
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
    let mut word_process: Option<u32> = None;
    loop {
        while let Ok(line) = lines.try_recv() {
            if let Some(pid) = helper(&line) {
                word_process = Some(pid);
            }
            seen.push(line);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // the last lines may still be on their way (but word may keep the output open)
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
        let cancelled = cancel.is_cancelled();
        if cancelled || started.elapsed() > limit {
            if word_process.is_none() {
                // stopping the script before it named its word copy would
                // leave that copy running unseen (com starts it, so it is
                // not the script's child): give it a moment to say
                word_process = wait_for_helper(&lines, &helper, &mut seen, helper_grace);
            }
            stop_process_tree(&mut child, word_process);
            while let Ok(line) = lines.try_recv() {
                seen.push(line);
            }
            return if cancelled {
                RunEnd::Cancelled { lines: seen }
            } else {
                RunEnd::TimedOut { lines: seen }
            };
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn wait_for_helper(
    lines: &mpsc::Receiver<String>,
    helper: &impl Fn(&str) -> Option<u32>,
    seen: &mut Vec<String>,
    grace: Duration,
) -> Option<u32> {
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        match lines.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                let pid = helper(&line);
                // "shared=" or "error=" mean no copy of ours exists
                let finished = line.starts_with("shared=") || line.starts_with("error=");
                seen.push(line);
                if pid.is_some() {
                    return pid;
                }
                if finished {
                    return None;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
    None
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

/// stops the script, everything it started, and the word process it named
/// (which, being started through com, isn't its child).
fn stop_process_tree(child: &mut Child, word_process: Option<u32>) {
    let mut kill = Command::new("taskkill");
    kill.args(["/PID", &child.id().to_string(), "/T", "/F"]);
    hide_window(&mut kill);
    let _ = kill.output();
    if let Some(pid) = word_process {
        end_word_process(pid);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// whether a process with this id is running and is word.
pub(crate) fn is_word_process(pid: u32) -> bool {
    let mut list = Command::new("tasklist");
    list.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]);
    hide_window(&mut list);
    list.output()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .to_uppercase()
                .contains("\"WINWORD.EXE\"")
        })
        .unwrap_or(false)
}

/// ends exactly this word process, after checking it still is word.
pub(crate) fn end_word_process(pid: u32) {
    if !is_word_process(pid) {
        return;
    }
    let mut kill = Command::new("taskkill");
    kill.args(["/PID", &pid.to_string(), "/F"]);
    hide_window(&mut kill);
    let _ = kill.output();
}

/// waits for our word process to quit on its own, then ends it if it didn't.
/// returns whether it is gone.
pub(crate) fn make_sure_word_exits(pid: u32, grace: Duration) -> bool {
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if !is_word_process(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    end_word_process(pid);
    std::thread::sleep(Duration::from_millis(500));
    !is_word_process(pid)
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
        // the helper names no word process: nothing but the script is stopped
        let end = run_with_limit(
            slow_command(),
            Duration::from_secs(3),
            &CancelFlag::new(),
            |_| None,
            Duration::ZERO,
        );
        assert!(matches!(end, RunEnd::TimedOut { .. }));
        assert!(started.elapsed() < Duration::from_secs(15));
    }

    #[cfg(windows)]
    #[test]
    fn cancelling_stops_the_program() {
        let cancel = CancelFlag::new();
        cancel.cancel();
        let end = run_with_limit(
            slow_command(),
            Duration::from_secs(60),
            &cancel,
            |_| None,
            Duration::ZERO,
        );
        assert!(matches!(end, RunEnd::Cancelled { .. }));
    }

    #[test]
    fn missing_programs_are_reported() {
        let end = run_with_limit(
            Command::new("letters-no-such-program"),
            Duration::from_secs(5),
            &CancelFlag::new(),
            |_| None,
            Duration::ZERO,
        );
        assert!(matches!(end, RunEnd::NotStarted(_)));
    }

    #[cfg(windows)]
    #[test]
    fn a_process_that_is_not_word_is_never_ended() {
        // this test's own process is not word, so it must survive
        end_word_process(std::process::id());
        assert!(!is_word_process(std::process::id()));
    }
}
