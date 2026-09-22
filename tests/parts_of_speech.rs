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
        .arg("parts-of-speech")
        .env_remove("TYPESAFE_API_KEY")
        .env_remove("OPENROUTER_API_KEY")
        .env_remove("TYPESAFE_MODEL");
    command
}

fn input(mut command: Command, text: &str) -> Output {
    let mut child = command
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
    child.wait_with_output().unwrap()
}

#[test]
fn request_preview_reads_files_without_credentials_and_has_no_duplicate_text() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("input.txt");
    std::fs::write(&file, "Birds fly. Cats sleep!").unwrap();
    let output = command()
        .arg(file)
        .arg("--request")
        .arg("--cost")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        body["state"]["sentences"],
        json!([["Birds", "fly", "."], ["Cats", "sleep", "!"]])
    );
    assert_eq!(body["questions"].as_object().unwrap().len(), 8);
    assert_eq!(
        body["questions"]["p(1,1)"]["instructions"],
        "p: `sentences[1][1]`"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout)
            .matches("Birds")
            .count(),
        1
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "Cost: 0.0000¢\nTokens: 0\nLuna Cost: 0.0000¢\n"
    );
}

#[test]
fn punctuation_is_local_and_empty_input_is_rejected() {
    let output = input(command(), ", ... ! … ?");
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    let tokens: Vec<_> = body["sentences"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|sentence| sentence.as_array().unwrap())
        .collect();
    assert_eq!(
        tokens
            .iter()
            .map(|token| token["text"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [",", "...", "!", "…", "?"]
    );
    assert!(tokens.iter().all(|token| token.get("confidence").is_none()));
    let output = input(command(), " \n\t");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("input text is empty"));
}

#[test]
fn one_batched_request_validates_all_word_answers_before_output() {
    for missing in [false, true] {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", server.server_addr());
        let worker = thread::spawn(move || {
            let mut request = server
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap();
            let body: Value = serde_json::from_reader(request.as_reader()).unwrap();
            assert_eq!(
                body["state"]["sentences"],
                json!([["I", "see", "you", "!"]])
            );
            let questions = body["questions"].as_object().unwrap();
            assert_eq!(questions.len(), 3);
            let mut answers = serde_json::Map::new();
            for (id, question) in questions {
                let function = &id[..1];
                let word = if id.ends_with("1)") { 1 } else { 2 };
                assert_eq!(
                    question["instructions"],
                    format!("{function}: `sentences[0][{word}]`")
                );
                assert_eq!(question["type"], "choice");
                let criteria = question["criteria"].as_object().unwrap();
                assert!(criteria.values().all(Value::is_null));
                let choice = match id.as_str() {
                    "p(0,1)" => "VBP",
                    "r(0,1)" => "VERB",
                    "r(0,2)" => "OBJ",
                    _ => panic!("unexpected target {id}"),
                };
                let probabilities: serde_json::Map<String, Value> = criteria
                    .keys()
                    .map(|key| (key.clone(), json!(if key == choice { 1.0 } else { 0.0 })))
                    .collect();
                answers.insert(
                    id.clone(),
                    json!({"type": "choice", "choice": choice,
                    "confidence": 1.0, "probabilities": probabilities}),
                );
            }
            if missing {
                answers.remove("r(0,1)");
            }
            request
                .respond(tiny_http::Response::from_string(
                    json!({"answers": answers,
                "usage": {"input_tokens": 1000}})
                    .to_string(),
                ))
                .unwrap();
        });
        let mut cmd = command();
        cmd.arg("-")
            .arg("--cost")
            .env("TYPESAFE_API_KEY", "test-key")
            .env("TYPESAFE_ENDPOINT", endpoint);
        let output = input(cmd, "I see you!");
        worker.join().unwrap();
        assert_eq!(output.status.success(), !missing);
        if missing {
            assert!(output.stdout.is_empty());
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("missing Choice answer for r(0,1)")
            );
        } else {
            let body: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                body["sentences"][0][0]["part_of_speech"],
                "personal pronoun"
            );
            assert_eq!(
                body["sentences"][0][0]["role"],
                "subject (head of a subject phrase)"
            );
            assert_eq!(body["sentences"][0][1]["confidence"]["role"], 1.0);
            assert_eq!(
                body["sentences"][0][0]["confidence"],
                json!({"part_of_speech": null, "role": null})
            );
            assert_eq!(
                body["sentences"][0][2]["confidence"],
                json!({"part_of_speech": null, "role": 1.0})
            );
            assert_eq!(body["sentences"][0][2]["role"], "direct object (head)");
            assert_eq!(
                body["sentences"][0][3]["part_of_speech"],
                "exclamation mark"
            );
        }
        assert!(String::from_utf8_lossy(&output.stderr).contains("Tokens: 1000\nLuna Cost:"));
    }
}

#[test]
fn common_words_need_no_credentials_or_api_usage() {
    let mut cmd = command();
    cmd.arg("--cost");
    let output = input(cmd, "The and a I an or she my");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    let words = body["sentences"][0].as_array().unwrap();
    assert_eq!(words.len(), 8);
    assert_eq!(words[0]["part_of_speech"], "article (a, an, the)");
    assert_eq!(words[1]["part_of_speech"], "coordinating conjunction");
    assert_eq!(words[3]["role"], "subject (head of a subject phrase)");
    assert!(
        words
            .iter()
            .all(|word| word["confidence"]["part_of_speech"].is_null()
                && word["confidence"]["role"].is_null())
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "Cost: 0.0000¢\nTokens: 0\nLuna Cost: 0.0000¢\n"
    );
}
