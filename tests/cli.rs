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
        .env_remove("OPENROUTER_API_KEY")
        .env_remove("OPENROUTER_ENDPOINT")
        .env_remove("TYPESAFE_API_KEY")
        .env_remove("TYPESAFE_ENDPOINT");
    command
}

#[test]
fn openrouter_key_uses_decisions_api_and_openrouter_model_name() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/api/alpha/decisions", server.server_addr());
    let worker = thread::spawn(move || {
        let mut request = server
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .expect("CLI should send an OpenRouter request");
        assert_eq!(request.url(), "/api/alpha/decisions");
        assert!(request.headers().iter().any(|header| {
            header.field.equiv("Authorization") && header.value.as_str() == "Bearer openrouter-key"
        }));
        let body: Value = serde_json::from_reader(request.as_reader()).unwrap();
        assert_eq!(body["model"], "typesafe/jev-1.13");
        request
            .respond(tiny_http::Response::from_string(answer(0.5).to_string()))
            .unwrap();
    });
    let mut child = command()
        .arg("phi")
        .env("OPENROUTER_API_KEY", "openrouter-key")
        .env("OPENROUTER_ENDPOINT", endpoint)
        .env("TYPESAFE_API_KEY", "unused-typesafe-key")
        .env("TYPESAFE_ENDPOINT", "http://127.0.0.1:1/unused")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"Sample text")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    worker.join().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        phi_output(0.5, "-")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn load_bearing_batches_lines_and_launches_files_concurrently() {
    let directory = tempfile::tempdir().unwrap();
    let source = "abc\r\n}\r\nconst value = 1;\r\nreturn value;\r\n";
    for name in ["a.ts", "b.ts"] {
        std::fs::write(directory.path().join(name), source).unwrap();
    }
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", server.server_addr());
    let worker = thread::spawn(move || {
        // Both requests must arrive before either receives a response.
        let mut requests = Vec::new();
        for _ in 0..2 {
            let mut request = server
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .expect("concurrent request");
            let body: Value = serde_json::from_reader(request.as_reader()).unwrap();
            assert_eq!(body["state"]["source"], source);
            assert_eq!(body["questions"].as_object().unwrap().len(), 2);
            assert!(
                body["state"]["definitions"]["a"]["instructions"]
                    .as_str()
                    .unwrap()
                    .contains("runtime behavior")
            );
            assert_eq!(
                body["state"]["definitions"]["a"]["criteria"]
                    .as_array()
                    .unwrap()
                    .len(),
                5
            );
            for line in ["3", "4"] {
                assert_eq!(body["questions"][line]["type"], "score");
                assert_eq!(
                    body["questions"][line]["instructions"],
                    format!("a({line},{line})")
                );
                for level in 0..5 {
                    assert_eq!(
                        body["questions"][line]["criteria"][level],
                        format!("`definitions.a.criteria[{level}]`")
                    );
                }
            }
            requests.push(request);
        }
        for request in requests {
            request.respond(tiny_http::Response::from_string(json!({"answers": {"3": {"type": "score", "score": 4}, "4": {"type": "score", "score": 2}}, "usage": {"input_tokens": 1000}}).to_string())).unwrap();
        }
    });
    let output = command()
        .arg("--cost")
        .arg("load-bearing")
        .arg(directory.path())
        .env("TYPESAFE_API_KEY", "test-key")
        .env("TYPESAFE_ENDPOINT", endpoint)
        .output()
        .unwrap();
    worker.join().unwrap();
    assert!(output.status.success(), "{:?}", output);
    let records: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 2);
    assert_ne!(records[0]["file"], records[1]["file"]);
    for record in records {
        assert_eq!(record["source"], source);
        assert_eq!(record["score"], json!({"3": 1.0, "4": 0.5}));
    }
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "Cost: 0.0084¢\nTokens: 2000\nLuna Cost: 0.0453¢ (5.4× Jev)\n"
    );
}

#[test]
fn load_bearing_skips_short_lines_without_credentials() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "abc\n}\néééé\n").unwrap();
    let output = command()
        .arg("load-bearing")
        .arg(file.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["score"], json!({}));
    assert!(output.stderr.is_empty());
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
        for metric in [
            "identifiability",
            "health_condition",
            "healthcare_provision",
            "healthcare_payment",
        ] {
            assert_eq!(body["questions"][metric]["type"], "noul");
            assert!(body["questions"][metric]["criteria"]["true"].is_string());
            assert!(body["questions"][metric]["criteria"]["false"].is_string());
        }
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
        .arg("--cost")
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
    json!({"answers": phi_answers(value), "usage": {"input_tokens": 1000, "output_tokens": 9000}})
}

fn phi_answers(value: f64) -> Value {
    json!({
        "identifiability": {"type": "noul", "noul": value},
        "health_condition": {"type": "noul", "noul": value},
        "healthcare_provision": {"type": "noul", "noul": value},
        "healthcare_payment": {"type": "noul", "noul": value}
    })
}

const PHI_LABELS: [&str; 4] = [
    "identifying information",
    "health condition",
    "healthcare provision",
    "healthcare payment",
];

fn phi_output(value: f64, name: &str) -> String {
    let source = if name == "-" {
        String::new()
    } else {
        format!(" {name}")
    };
    PHI_LABELS
        .iter()
        .map(|metric| format!("{value:.2} {metric}{source}\n"))
        .collect()
}

#[test]
fn stdin_and_explicit_dash_return_noul_and_name() {
    for args in [vec!["phi"], vec!["phi", "-"]] {
        let output = score(&args, "Jane Doe has diabetes.\n", answer(0.97), 200);
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "Cost: 0.0042¢\nTokens: 1000\nLuna Cost: 0.0282¢ (6.7× Jev)\n"
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            phi_output(0.97, "-")
        );
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
        phi_output(0.02, file.path().file_name().unwrap().to_str().unwrap())
    );
}

#[test]
fn directory_scans_files_with_colors_and_continues_after_errors() {
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
        for _ in 0..4 {
            let mut request = server
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap();
            let body: Value = serde_json::from_reader(request.as_reader()).unwrap();
            let probability = match body["state"]["text"].as_str().unwrap() {
                "a.txt" => 0.1,
                "b.txt" => 0.2,
                "c.txt" => 0.97,
                "z.txt" => 0.0,
                name => panic!("unexpected file: {name}"),
            };
            request
                .respond(tiny_http::Response::from_string(
                    answer(probability).to_string(),
                ))
                .unwrap();
        }
    });
    let output = command()
        .arg("--cost")
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
    let output_text = String::from_utf8(output.stdout).unwrap();
    let mut lines: Vec<_> = output_text.lines().map(str::to_owned).collect();
    lines.sort();
    let mut expected = Vec::new();
    for (value, code, name) in [
        (0.1, 31, "a.txt"),
        (0.2, 32, "b.txt"),
        (0.97, 32, "c.txt"),
        (0.0, 31, "z.txt"),
    ] {
        for metric in PHI_LABELS {
            expected.push(format!("\x1b[1;{code}m{value:.2}\x1b[0m {metric} {name}"));
        }
    }
    expected.sort();
    assert_eq!(lines, expected);
    assert!(String::from_utf8_lossy(&output.stderr).contains("empty.txt: input text is empty"));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Cost: 0.0168¢\nTokens: 4000\nLuna Cost: ")
    );
}

#[test]
fn usage_is_counted_even_when_the_answer_is_invalid() {
    let output = score(&["phi"], "Sample text", answer(1.1), 200);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .ends_with("Cost: 0.0042¢\nTokens: 1000\nLuna Cost: 0.0282¢ (6.7× Jev)\n")
    );
}

#[test]
fn missing_usage_preserves_answers_but_reports_unknown_cost() {
    let output = score(
        &["phi"],
        "Sample text",
        json!({"answers": phi_answers(0.7)}),
        200,
    );
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        phi_output(0.7, "-")
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "Cost: unavailable (known cost: 0.0000¢; token usage missing for 1 request(s))\n\
         Tokens: unavailable (known tokens: 0; usage missing for 1 request(s))\n\
         Luna Cost: unavailable (known cost: 0.0000¢; input usage or answers missing for 1 request(s))\n"
    );
}

#[test]
fn phi_launches_all_requests_and_streams_completed_results_before_slower_files() {
    use std::io::{BufRead, BufReader};
    let directory = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(directory.path().join(name), name).unwrap();
    }
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let mut child = command()
        .arg("phi")
        .arg("--cost")
        .arg(directory.path())
        .env("TYPESAFE_API_KEY", "test-key")
        .env(
            "TYPESAFE_ENDPOINT",
            format!("http://{}/v1/systemone", server.server_addr()),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut pending = std::collections::HashMap::new();
    // A serial implementation cannot deliver all three requests before any response.
    for _ in 0..3 {
        let mut request = server
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .expect("all requests must be in flight concurrently");
        let body: Value = serde_json::from_reader(request.as_reader()).unwrap();
        pending.insert(body["state"]["text"].as_str().unwrap().to_owned(), request);
    }
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            sender.send(line.unwrap()).unwrap();
        }
    });
    // Hold the first file back, and require each other line to arrive before releasing it.
    for name in ["c.txt", "b.txt", "a.txt"] {
        assert!(child.try_wait().unwrap().is_none());
        pending
            .remove(name)
            .unwrap()
            .respond(tiny_http::Response::from_string(answer(0.7).to_string()))
            .unwrap();
        for metric in PHI_LABELS {
            assert_eq!(
                receiver
                    .recv_timeout(Duration::from_secs(10))
                    .expect("result must flush immediately"),
                format!("0.70 {metric} {name}")
            );
        }
    }
    let output = child.wait_with_output().unwrap();
    reader.join().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "Cost: 0.0126¢\nTokens: 3000\nLuna Cost: 0.0845¢ (6.7× Jev)\n"
    );
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
    assert!(
        String::from_utf8_lossy(&no_key.stderr)
            .contains("set OPENROUTER_API_KEY or TYPESAFE_API_KEY")
    );
}

#[test]
fn help_and_argument_errors() {
    for args in [
        vec!["--help"],
        vec!["phi", "--help"],
        vec!["business", "--help"],
        vec!["job", "--help"],
        vec!["--version"],
    ] {
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
            assert!(body["state"].get("comments").is_none());
            for function in ["a", "b"] {
                assert!(body["state"]["definitions"][function]["instructions"].is_string());
                assert_eq!(
                    body["state"]["definitions"][function]["criteria"]
                        .as_array()
                        .unwrap()
                        .len(),
                    4
                );
            }
            let questions = body["questions"].as_object().unwrap();
            assert_eq!(questions.len(), 4);
            let mut answers = serde_json::Map::new();
            for (id, question) in questions {
                assert_eq!(question["type"], "score");
                assert_eq!(question["criteria"].as_array().unwrap().len(), 4);
                let range = if id.contains("_0_") { "1,2" } else { "4,4" };
                let function = if id.ends_with("accurate") { "a" } else { "b" };
                assert_eq!(question["instructions"], format!("{function}({range})"));
                for level in 0..4 {
                    assert_eq!(
                        question["criteria"][level],
                        format!("`definitions.{function}.criteria[{level}]`")
                    );
                }
                let value = if id.ends_with("accurate") { 2.37 } else { 0.24 };
                answers.insert(
                    id.clone(),
                    json!({"type": "score", "score": value, "confidence": 0.9}),
                );
            }
            request
                .respond(tiny_http::Response::from_string(
                    json!({"answers": answers, "usage": {"input_tokens": 1000, "output_tokens": 9000}}).to_string(),
                ))
                .unwrap();
        });
        let mut command = command();
        command.args(["code-comments", "--model", "test-model", "--cost"]);
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
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "Cost: 0.0042¢\nTokens: 1000\nLuna Cost: 0.0313¢ (7.4× Jev)\n"
        );
        let text = String::from_utf8(output.stdout).unwrap();
        let name = if matches!(mode, "file" | "directory") {
            "example.ts"
        } else {
            "-"
        };
        assert!(text.starts_with(&format!("{name}:1-2\nAccuracy:   ")));
        assert!(text.contains(&format!("{name}:4\n")));
        assert_eq!(text.matches("79/100").count(), 2);
        assert_eq!(text.matches("8/100").count(), 2);
        assert_eq!(text.matches("79/100 · Mostly accurate").count(), 2);
        assert_eq!(text.matches("8/100 · No useful information").count(), 2);
        assert_eq!(text.matches("Usefulness:").count(), 2);
        assert!(text.contains("│ // Keep the historical delay. │"));
        assert!(text.contains("│ // Old clients depend on it.  │"));
        assert!(text.contains("│ wait(100);"));
        assert!(text.contains("│ count++;"));
        assert!(text.contains('╭') && text.contains('╯'));
        assert!(!text.contains('\x1b'));
    }
}

#[test]
fn comment_free_files_need_no_api_key() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"const value = `/* not a comment */`;\n")
        .unwrap();
    let output = command()
        .arg("code-comments")
        .arg(file.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn code_comments_reject_invalid_scores_without_partial_output() {
    for invalid in [
        json!({"type": "score", "score": -0.1}),
        json!({"type": "score", "score": 3.01}),
        json!({"type": "noul", "noul": 0.8}),
        json!({"type": "score"}),
    ] {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"// Increment the counter.\ncount++;\n")
            .unwrap();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/systemone", server.server_addr());
        let worker = thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap();
            request
                .respond(tiny_http::Response::from_string(
                    json!({
                        "answers": {
                            "comment_0_accurate": {"type": "score", "score": 3.0},
                            "comment_0_useful": invalid
                        },
                        "usage": {"input_tokens": 1000}
                    })
                    .to_string(),
                ))
                .unwrap();
        });
        let output = command()
            .arg("code-comments")
            .arg("--cost")
            .arg(file.path())
            .env("TYPESAFE_API_KEY", "test-key")
            .env("TYPESAFE_ENDPOINT", endpoint)
            .output()
            .unwrap();
        worker.join().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("Cost: 0.0042¢\nTokens: 1000\nLuna Cost: ")
        );
    }
}
