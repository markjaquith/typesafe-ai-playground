use std::{
    io::Write,
    process::{Command, Output, Stdio},
    thread,
    time::Duration,
};

use serde_json::{Value, json};

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_typesafe-ai"));
    command
        .env_remove("TYPESAFE_MODEL")
        .env_remove("CLICOLOR_FORCE")
        .env("NO_COLOR", "1")
        .env_remove("TYPESAFE_API_KEY")
        .env_remove("TYPESAFE_ENDPOINT");
    command
}

fn score(args: &[&str], text: &str, response: Value, status: u16) -> Output {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/systemone", server.server_addr());
    let expected_text = text.to_owned();
    let worker = thread::spawn(move || {
        let mut request = server
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .expect("CLI should send a request");
        assert_eq!(request.method(), &tiny_http::Method::Post);
        assert_eq!(request.url(), "/v1/systemone");
        assert!(request.headers().iter().any(|header| {
            header.field.equiv("Authorization") && header.value.as_str() == "Bearer test-key"
        }));
        let body: Value = serde_json::from_reader(request.as_reader()).unwrap();
        assert_eq!(body["state"]["text"], expected_text);
        assert_eq!(body["model"], "jev-latest");
        assert_eq!(body["questions"]["phi"]["type"], "noul");
        request
            .respond(
                tiny_http::Response::from_string(response.to_string())
                    .with_status_code(status)
                    .with_header(
                        tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ),
            )
            .unwrap();
    });
    let mut child = command()
        .args(args)
        .env("TYPESAFE_API_KEY", "test-key")
        .env("TYPESAFE_ENDPOINT", endpoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    worker.join().unwrap();
    output
}

fn answer(value: f64) -> Value {
    json!({"answers": {"phi": {"type": "noul", "noul": value}}})
}

#[test]
fn stdin_and_explicit_dash_return_noul_and_name() {
    for args in [vec!["phi"], vec!["phi", "-"]] {
        let output = score(&args, "Jane Doe has diabetes.\n", answer(0.97), 200);
        assert!(output.status.success(), "{:?}", output);
        assert!(output.stderr.is_empty());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "0.97 -\n");
    }
}

#[test]
fn file_input_is_sent_verbatim() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    let text = "General health advice: drink water.\n";
    file.write_all(text.as_bytes()).unwrap();
    let output = score(
        &["phi", file.path().to_str().unwrap()],
        text,
        answer(0.02),
        200,
    );
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "0.02 {}\n",
            file.path().file_name().unwrap().to_str().unwrap()
        )
    );
}

#[test]
fn directory_scans_files_in_order_with_colors_and_continues_after_errors() {
    let directory = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(directory.path().join(name), name).unwrap();
    }
    std::fs::write(directory.path().join("empty.txt"), "").unwrap();
    std::fs::create_dir(directory.path().join("nested")).unwrap();
    std::fs::write(directory.path().join("nested/ignored.txt"), "nested").unwrap();
    std::fs::write(directory.path().join("z.txt"), "z.txt").unwrap();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/systemone", server.server_addr());
    let worker = thread::spawn(move || {
        for (name, probability) in [
            ("a.txt", 0.1),
            ("b.txt", 0.2),
            ("c.txt", 0.97),
            ("z.txt", 0.0),
        ] {
            let mut request = server
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap();
            let body: Value = serde_json::from_reader(request.as_reader()).unwrap();
            assert_eq!(body["state"]["text"], name);
            request
                .respond(tiny_http::Response::from_string(
                    answer(probability).to_string(),
                ))
                .unwrap();
        }
    });
    let output = command()
        .arg("phi")
        .arg(directory.path())
        .env("TYPESAFE_API_KEY", "test-key")
        .env("TYPESAFE_ENDPOINT", endpoint)
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .output()
        .unwrap();
    worker.join().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "\x1b[1;31m0.1\x1b[0m a.txt\n\x1b[1;32m0.2\x1b[0m b.txt\n\x1b[1;32m0.97\x1b[0m c.txt\n\x1b[1;31m0\x1b[0m z.txt\n"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("empty.txt: input text is empty"));
}

#[test]
fn rejects_invalid_responses_and_http_errors() {
    for (response, status) in [
        (answer(1.1), 200),
        (json!({"answers": {}}), 200),
        (
            json!({"answers": {"phi": {"type": "score", "noul": 0.8}}}),
            200,
        ),
        (json!({"error": "secret input should not be echoed"}), 401),
    ] {
        let output = score(&["phi"], "Sample text", response, status);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.starts_with("error:"));
        assert!(!error.contains("secret input"));
    }
}

#[test]
fn input_and_auth_errors_are_actionable() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("text.txt");
    let missing = command().arg("phi").arg(&file).output().unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("could not read"));

    std::fs::write(&file, "  \n").unwrap();
    let empty = command().arg("phi").arg(&file).output().unwrap();
    assert!(!empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stderr).contains("input text is empty"));

    std::fs::write(&file, "Some text").unwrap();
    let no_key = command().arg("phi").arg(&file).output().unwrap();
    assert!(!no_key.status.success());
    assert!(String::from_utf8_lossy(&no_key.stderr).contains("set TYPESAFE_API_KEY"));
}

#[test]
fn help_and_argument_errors() {
    for args in [vec!["--help"], vec!["phi", "--help"], vec!["--version"]] {
        let output = command().args(args).output().unwrap();
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    let output = command().args(["phi", "--unknown"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn code_comments_batch_two_questions_per_span_for_file_directory_and_stdin() {
    let source = "// Keep the historical delay.\n// Old clients depend on it.\nwait(100);\n/* Increment the counter. */\ncount++;\n";
    for mode in ["file", "directory", "stdin", "dash"] {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("example.ts");
        std::fs::write(&file, source).unwrap();
        std::fs::write(
            directory.path().join("no-comments.ts"),
            "const url = 'https://example.com';",
        )
        .unwrap();
        std::fs::create_dir(directory.path().join("nested")).unwrap();
        std::fs::write(directory.path().join("nested/ignored.ts"), "// ignored").unwrap();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/systemone", server.server_addr());
        let worker = thread::spawn(move || {
            let mut request = server
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap();
            let body: Value = serde_json::from_reader(request.as_reader()).unwrap();
            assert_eq!(body["state"]["source"], source);
            assert_eq!(body["model"], "test-model");
            let comments = body["state"]["comments"].as_array().unwrap();
            assert_eq!(comments.len(), 2);
            assert_eq!(comments[0]["start_line"], 1);
            assert_eq!(comments[0]["end_line"], 2);
            assert_eq!(comments[1]["start_line"], 4);
            let questions = body["questions"].as_object().unwrap();
            assert_eq!(questions.len(), 4);
            let mut answers = serde_json::Map::new();
            for (id, question) in questions {
                assert_eq!(question["type"], "noul");
                let index = if id.contains("_0_") { 0 } else { 1 };
                assert!(
                    question["instructions"]
                        .as_str()
                        .unwrap()
                        .contains(&format!("comments[{index}].comment"))
                );
                let value = if id.ends_with("accurate") { 0.79 } else { 0.08 };
                answers.insert(id.clone(), json!({"type": "noul", "noul": value}));
            }
            request
                .respond(tiny_http::Response::from_string(
                    json!({"answers": answers}).to_string(),
                ))
                .unwrap();
        });
        let mut command = command();
        command.args(["code-comment", "--model", "test-model"]);
        match mode {
            "file" => {
                command.arg(&file);
            }
            "directory" => {
                command.arg(directory.path());
            }
            "dash" => {
                command.arg("-");
            }
            _ => {}
        }
        let mut child = command
            .env("TYPESAFE_API_KEY", "test-key")
            .env("TYPESAFE_ENDPOINT", endpoint)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        worker.join().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        let name = if matches!(mode, "file" | "directory") {
            "example.ts"
        } else {
            "-"
        };
        assert!(text.starts_with(&format!("{name}:1-2\nAccurate:  ")));
        assert!(text.contains(&format!("{name}:4-4\n")));
        assert_eq!(text.matches("79%").count(), 2);
        assert_eq!(text.matches("8%").count(), 2);
        assert!(
            text.contains("// Keep the historical delay.\n// Old clients depend on it.\n\n---")
        );
        assert!(!text.contains('\x1b'));
    }
}

#[test]
fn comment_free_files_need_no_api_key() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"const value = `/* not a comment */`;\n")
        .unwrap();
    let output = command()
        .arg("code-comment")
        .arg(file.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
