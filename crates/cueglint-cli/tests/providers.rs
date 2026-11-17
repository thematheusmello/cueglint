use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    process::{Command, Output},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

fn run(database: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cueglint-cli"))
        .arg("--db")
        .arg(database)
        .args(args)
        .output()
        .unwrap()
}

fn succeeded(output: Output) -> Value {
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        !error.contains("native credential store is unavailable"),
        "BLOCKED: the OS credential store could not read the unique test-only endpoint reference. No key mutation or unrelated credential lookup was attempted."
    );
    assert!(output.status.success(), "CLI fixture failed: {error}");
    serde_json::from_slice(&output.stdout).unwrap()
}

fn answer() -> String {
    format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"model": "original-cli-model", "choices": [{"delta": {"content": "Original CLI streamed answer"}}]}),
        json!({"model": "original-cli-model", "choices": [{"delta": {}, "finish_reason": "stop"}]})
    )
}

#[test]
fn actual_cli_routes_only_explicit_model_test_and_answer_actions_to_a_unique_loopback_endpoint() {
    let directory = std::env::temp_dir().join(format!("cueglint-cli-provider-{}", Uuid::new_v4()));
    let database = directory.join("state.sqlite3");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    // The whole canonical API base participates in the key reference; this namespace can never be a user's saved endpoint.
    let api_path = format!("/cueglint-test-{}/v1/", Uuid::new_v4());
    let endpoint = format!("http://{}{api_path}", listener.local_addr().unwrap());
    let (tx, requests) = mpsc::channel();
    let server = thread::spawn(move || {
        for (status, content_type, body) in [
            (
                200,
                "application/json",
                json!({"data": [{"id": "original-cli-model"}]}).to_string(),
            ),
            (200, "text/event-stream", answer()),
            (
                500,
                "application/json",
                json!({"error": {"message": "original error body must remain private"}})
                    .to_string(),
            ),
            (200, "text/event-stream", answer()),
        ] {
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("loopback CLI fixture accept failed: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut incoming = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                incoming.extend_from_slice(&buffer[..count]);
                assert!(incoming.len() < 128 * 1024);
                if let Some(start) = incoming.windows(4).position(|value| value == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&incoming[..start]).unwrap();
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.split_once(':')
                                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        })
                        .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if incoming.len() >= start + 4 + length {
                        break;
                    }
                }
            }
            tx.send(String::from_utf8(incoming).unwrap()).unwrap();
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    assert_eq!(
        succeeded(run(&database, &["provider", "show"]))["provider"],
        "local"
    );
    let selected = succeeded(run(
        &database,
        &[
            "provider",
            "select",
            "compatible",
            "--model",
            "original-cli-model",
            "--base-url",
            &endpoint,
        ],
    ));
    assert_eq!(selected["provider"], "open_ai_compatible");
    assert!(
        requests.try_recv().is_err(),
        "saving selection must not contact the endpoint"
    );
    let session = succeeded(run(
        &database,
        &["session", "start", "[Fixture] CLI provider"],
    ));
    let id = session["id"].as_str().unwrap();
    const CONTEXT: &str = "original-fixture-context-not-for-connection-test";
    succeeded(run(&database, &["transcript", id, CONTEXT]));
    succeeded(run(
        &database,
        &[
            "session",
            "notes",
            id,
            "Original notes survive remote errors",
        ],
    ));
    let catalog = succeeded(run(&database, &["provider", "models"]));
    assert_eq!(catalog["models"][0]["id"], "original-cli-model");
    let listing = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(listing.starts_with(&format!("GET {api_path}models ")));
    assert!(!listing.contains(CONTEXT));
    let test = succeeded(run(&database, &["provider", "test"]));
    assert_eq!(test["connected"], true);
    assert_eq!(test["meeting_context_sent"], false);
    let connection = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(connection.contains("Reply with the single word OK."));
    assert!(
        !connection.contains(CONTEXT)
            && !connection.to_ascii_lowercase().contains("authorization:")
    );
    let failed = run(
        &database,
        &["ask", id, "Original deliberate question", "--stream"],
    );
    assert!(!failed.status.success());
    assert!(
        !String::from_utf8_lossy(&failed.stderr)
            .contains("original error body must remain private")
    );
    requests.recv_timeout(Duration::from_secs(1)).unwrap();
    let after_failure = succeeded(run(&database, &["session", "show", id]));
    assert_eq!(after_failure["requests"][0]["status"], "cancelled");
    assert_eq!(after_failure["suggestions"], json!([]));
    let answer_output = run(
        &database,
        &["ask", id, "Original deliberate question", "--stream"],
    );
    assert!(
        String::from_utf8_lossy(&answer_output.stderr).contains("Original CLI streamed answer")
    );
    let answer = succeeded(answer_output);
    assert_eq!(answer["applied"], true);
    assert_eq!(
        answer["output"]["metadata"]["requested_model"],
        "original-cli-model"
    );
    assert!(answer["output"]["metadata"]["model_version"].is_null());
    assert!(answer["output"]["metadata"]["usage"]["input_tokens"].is_null());
    let sent = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(sent.starts_with(&format!("POST {api_path}chat/completions ")));
    assert!(sent.contains(CONTEXT));
    let saved = succeeded(run(&database, &["session", "show", id]));
    assert_eq!(saved["requests"].as_array().unwrap().len(), 2);
    assert_eq!(saved["requests"][1]["status"], "completed");
    assert!(saved["suggestions"][0]["inference"].is_null());
    assert_eq!(
        saved["suggestions"][0]["hosted_inference"]["provider"],
        "open_ai_compatible"
    );
    assert_eq!(saved["notes"], "Original notes survive remote errors");
    assert!(
        requests.try_recv().is_err(),
        "the CLI must never automatically retry or fall back"
    );
    server.join().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
