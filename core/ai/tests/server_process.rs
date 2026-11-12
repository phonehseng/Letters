//! starting, using and stopping a model server, with a stand-in program
//! (`tests/support/stub_server.rs`) in place of a real llama.cpp server.

use std::path::PathBuf;
use std::time::Duration;

use letters_ai::{
    AiError, AiTools, CancelToken, Document, LaunchOptions, LocalServer, Page, Paragraph, Provider,
    Rect, ServerProcess, SummaryLength, TaskOutput, TaskRequest,
};

const STUB: &str = env!("CARGO_BIN_EXE_letters-ai-stub-server");

/// writes a fake model file whose contents tell the stub how to behave.
fn model_file(name: &str, behaviour: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("letters-ai-test-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(name);
    std::fs::write(&path, behaviour).unwrap();
    path
}

fn options(model: PathBuf) -> LaunchOptions {
    LaunchOptions {
        startup_timeout: Duration::from_secs(20),
        ..LaunchOptions::new(STUB, model)
    }
}

fn one_page() -> Document {
    Document {
        pages: vec![Page {
            number: 1,
            paragraphs: vec![Paragraph {
                text: "A short page about cells.".into(),
                rect: Rect::default(),
            }],
        }],
    }
}

#[test]
fn starts_answers_and_stops() {
    let model = model_file("ready.bin", "ready");
    let mut server = ServerProcess::start(&options(model), &CancelToken::new()).unwrap();
    assert!(server.is_running());
    assert!(server.address().starts_with("http://127.0.0.1:"));

    let provider = LocalServer::new(&server.config()).unwrap();
    assert!(provider.is_ready());
    assert!(!provider.describe().sends_content_off_device);

    let tools = AiTools::with_provider(Box::new(provider));
    let plan = tools
        .plan(
            &one_page(),
            TaskRequest::Summarize {
                pages: None,
                length: SummaryLength::Short,
            },
        )
        .unwrap();
    assert_eq!(plan.disclosure().sentence(), "This stays on this computer.");
    let output = tools
        .run(plan, None, &mut |_| {}, &CancelToken::new())
        .unwrap();
    let TaskOutput::Summary(summary) = output else {
        panic!("expected a summary")
    };
    assert_eq!(summary.text, "Hello from the stub.");

    let port = server.port();
    server.stop();
    // the port is free again once the process has gone
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
}

#[test]
fn waits_while_the_model_loads() {
    let model = model_file("slow.bin", "slow");
    let server = ServerProcess::start(&options(model), &CancelToken::new()).unwrap();
    assert!(LocalServer::new(&server.config()).unwrap().is_ready());
}

#[test]
fn a_server_that_quits_while_starting_is_reported() {
    let model = model_file("broken.bin", "exit");
    let error = ServerProcess::start(&options(model), &CancelToken::new()).err();
    assert_eq!(error, Some(AiError::ServerExited));
}

#[test]
fn stop_while_starting_ends_the_process() {
    let model = model_file("slow-cancel.bin", "slow");
    let cancel = CancelToken::new();
    cancel.cancel();
    let error = ServerProcess::start(&options(model), &cancel).err();
    assert_eq!(error, Some(AiError::Cancelled));
}

#[test]
fn dropping_the_handle_stops_the_server() {
    let model = model_file("drop.bin", "ready");
    let server = ServerProcess::start(&options(model), &CancelToken::new()).unwrap();
    let port = server.port();
    drop(server);
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
}
