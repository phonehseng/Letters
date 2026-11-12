//! starting and stopping a llama.cpp server for the person.
//!
//! the person points Letters at two files they already have: the server
//! program and a model file. Letters starts the program on a free port on
//! 127.0.0.1 (so only this computer can reach it), waits until its health
//! check says the model is loaded, and stops it again when asked or when
//! the app quits. nothing is ever downloaded: only a local model file is
//! passed, never an address.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::cancel::CancelToken;
use crate::settings::LocalServerConfig;
use crate::wire::http::{HttpClient, ProxyUse, Timeouts};
use crate::AiError;

/// how often to ask whether the server is ready while it loads.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// what to start.
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchOptions {
    /// the llama.cpp server program.
    pub executable: PathBuf,
    /// the model file it should load.
    pub model_file: PathBuf,
    /// the context size to ask for, or the program's own default.
    pub context_tokens: Option<usize>,
    /// turn on the embeddings endpoint (better "find where").
    pub embeddings: bool,
    /// loading a big model from a slow disk can take a minute or more.
    pub startup_timeout: Duration,
}

impl LaunchOptions {
    pub fn new(executable: impl Into<PathBuf>, model_file: impl Into<PathBuf>) -> Self {
        LaunchOptions {
            executable: executable.into(),
            model_file: model_file.into(),
            context_tokens: None,
            embeddings: false,
            startup_timeout: Duration::from_secs(180),
        }
    }
}

/// a running server. stopping it, or dropping this, ends the process.
pub struct ServerProcess {
    child: Option<Child>,
    port: u16,
    context_tokens: usize,
    embeddings: bool,
}

impl ServerProcess {
    /// starts the server and waits until it is ready.
    pub fn start(options: &LaunchOptions, cancel: &CancelToken) -> Result<ServerProcess, AiError> {
        check_file(&options.executable, "model server program")?;
        check_file(&options.model_file, "model file")?;
        let port = free_port()?;
        let child = spawn(options, port)?;
        let mut process = ServerProcess {
            child: Some(child),
            port,
            context_tokens: options.context_tokens.unwrap_or(4096),
            embeddings: options.embeddings,
        };
        // on any failure the process is dropped here, which stops it
        process.wait_until_ready(options.startup_timeout, cancel)?;
        Ok(process)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// the address to put in the local server settings.
    pub fn address(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// settings for a `LocalServer` that talks to this process.
    pub fn config(&self) -> LocalServerConfig {
        LocalServerConfig {
            address: self.address(),
            context_tokens: self.context_tokens,
            embeddings: self.embeddings,
            ..LocalServerConfig::default()
        }
    }

    /// false once the process has ended, for whatever reason.
    pub fn is_running(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// ends the process and waits for it, so its port and the model
    /// file are free afterwards.
    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        if let Some(mut child) = self.child.take() {
            // the program holds no state worth saving, so ending it at
            // once is safe; waiting reaps it so no zombie is left behind
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn wait_until_ready(&mut self, timeout: Duration, cancel: &CancelToken) -> Result<(), AiError> {
        let health = HttpClient::new(
            "127.0.0.1",
            "",
            Timeouts {
                connect: Duration::from_secs(1),
                between_reads: Duration::from_secs(5),
            },
            ProxyUse::Never,
        );
        let url = format!("{}/health", self.address());
        let started = Instant::now();
        loop {
            cancel.check()?;
            if !self.is_running() {
                return Err(AiError::ServerExited);
            }
            // 503 means "still loading the model"; anything but 200 waits
            if let Ok(200) = health.get_status(&url) {
                return Ok(());
            }
            if started.elapsed() > timeout {
                return Err(AiError::ServerStartFailed(
                    "it didn’t become ready in time".to_string(),
                ));
            }
            thread::sleep(POLL_INTERVAL);
        }
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        self.shut_down();
    }
}

fn check_file(path: &Path, what: &str) -> Result<(), AiError> {
    // an address here could make the program fetch a model from the
    // internet, which this app never does
    let looks_like_address = path.to_string_lossy().contains("://");
    if looks_like_address || !path.is_file() {
        return Err(AiError::FileMissing(what.to_string()));
    }
    Ok(())
}

/// asks the system for a port nobody is using.
///
/// the port is released again straight away for the server to take. in
/// the moment between, another program could grab it; the health check
/// would then fail and the person can simply try again.
fn free_port() -> Result<u16, AiError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|_| AiError::ServerStartFailed("no free port was found".to_string()))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|_| AiError::ServerStartFailed("no free port was found".to_string()))
}

fn spawn(options: &LaunchOptions, port: u16) -> Result<Child, AiError> {
    let mut command = Command::new(&options.executable);
    command.arg("-m").arg(&options.model_file).args([
        "--host",
        "127.0.0.1",
        "--port",
        &port.to_string(),
    ]);
    if let Some(context) = options.context_tokens {
        command.args(["-c", &context.to_string()]);
    }
    if options.embeddings {
        command.arg("--embeddings");
    }
    // the server's own output isn't shown anywhere, and an unread pipe
    // would eventually fill up and freeze it
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console_window(&mut command);
    command
        .spawn()
        .map_err(|error| AiError::ServerStartFailed(spawn_reason(&error)))
}

fn spawn_reason(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => {
            "this computer didn’t allow the program to run".to_string()
        }
        std::io::ErrorKind::NotFound => "the program wasn’t found".to_string(),
        _ => "the program couldn’t be run".to_string(),
    }
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    // a console program started from a window app would otherwise flash
    // up a black console window
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_are_named() {
        let options = LaunchOptions::new("no-such-program.exe", "no-such-model.bin");
        let error = ServerProcess::start(&options, &CancelToken::new()).err();
        assert_eq!(
            error,
            Some(AiError::FileMissing("model server program".into()))
        );
    }

    #[test]
    fn a_model_address_is_refused() {
        let program = std::env::current_exe().unwrap();
        let options = LaunchOptions::new(program, "http://127.0.0.1:1/model.bin");
        let error = ServerProcess::start(&options, &CancelToken::new()).err();
        assert_eq!(error, Some(AiError::FileMissing("model file".into())));
    }

    #[test]
    fn free_ports_are_real_ports() {
        assert_ne!(free_port().unwrap(), 0);
    }
}
