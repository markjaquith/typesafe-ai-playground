use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

use serde_json::{Value, json};

struct Viewer(Child);

impl Drop for Viewer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start(endpoint: Option<&str>) -> (Viewer, String, reqwest::blocking::Client) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_typesafe-ai"));
    command
        .args([
            "parts-of-speech-serve",
            "--bind",
            "127.0.0.1:0",
            "--model",
            "test-model",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .env_remove("OPENROUTER_API_KEY")
        .env_remove("TYPESAFE_API_KEY");
    if let Some(endpoint) = endpoint {
        command
            .env("TYPESAFE_API_KEY", "test-key")
            .env("TYPESAFE_ENDPOINT", endpoint);
    }
    let mut viewer = Viewer(command.spawn().unwrap());
    let mut reader = BufReader::new(viewer.0.stderr.take().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let address = line
        .strip_prefix("Parts-of-speech viewer: ")
        .expect(&line)
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    (viewer, address, client)
}

#[test]
fn starts_without_input_handles_local_parsing_and_validation_and_recovers() {
    let (_viewer, address, client) = start(None);
    let html = client.get(&address).send().unwrap().text().unwrap();
    let doc = scraper::Html::parse_document(&html);
    let form = doc
        .select(&scraper::Selector::parse("#parse-form").unwrap())
        .next()
        .unwrap();
    assert!(form.value().attr("hidden").is_none());
    assert!(
        doc.select(&scraper::Selector::parse(".token").unwrap())
            .next()
            .is_none()
    );
    let endpoint = format!("{address}/api/parse");
    for body in [
        json!({"text":""}),
        json!({"text":" \n "}),
        json!({"text":"bad\0text"}),
        json!({"wrong":"field"}),
    ] {
        let response = client.post(&endpoint).json(&body).send().unwrap();
        assert_eq!(response.status().as_u16(), 400);
        assert!(response.json::<Value>().unwrap()["error"].is_string());
    }
    let response = client
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .body("{")
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);
    let response = client.post(&endpoint).body("text=I").send().unwrap();
    assert_eq!(response.status().as_u16(), 415);
    let response = client
        .post(&endpoint)
        .json(&json!({"text":"Birds fly."}))
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 502);
    assert!(
        response.json::<Value>().unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("API key")
    );
    // A failed parse must not take down the server or prevent a subsequent parse.
    let response = client
        .post(&endpoint)
        .json(&json!({"text":"I and she!"}))
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let body: Value = response.json().unwrap();
    assert_eq!(body["view"]["tokens"], 4);
    assert_eq!(body["view"]["local"], 4);
    assert_eq!(body["cost"], "Cost: 0.0000¢\nTokens: 0\nLuna Cost: 0.0000¢");
    assert!(
        body["view"]["content"]
            .as_str()
            .unwrap()
            .contains("personal pronoun")
    );
    assert_eq!(
        client
            .get(format!("{address}/missing"))
            .send()
            .unwrap()
            .status()
            .as_u16(),
        404
    );
}

#[test]
fn calls_jev_and_keeps_serving_while_parse_is_pending() {
    let api = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let api_endpoint = format!("http://{}", api.server_addr());
    let (arrived, received) = std::sync::mpsc::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let mut request = api.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
        let body: Value = serde_json::from_reader(request.as_reader()).unwrap();
        assert_eq!(body["model"], "test-model");
        assert_eq!(
            body["state"]["sentences"],
            json!([["I", "see", "you", "."]])
        );
        let questions = body["questions"].as_object().unwrap();
        assert_eq!(questions.len(), 3);
        arrived.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(10)).unwrap();
        let mut answers = serde_json::Map::new();
        for (id, question) in questions {
            let choice = match id.as_str() {
                "p(0,1)" => "VBP",
                "r(0,1)" => "VERB",
                "r(0,2)" => "OBJ",
                _ => panic!("unexpected question {id}"),
            };
            let probabilities: serde_json::Map<String, Value> = question["criteria"]
                .as_object()
                .unwrap()
                .keys()
                .map(|key| (key.clone(), json!(if key == choice { 1.0 } else { 0.0 })))
                .collect();
            answers.insert(id.clone(), json!({"type":"choice", "choice":choice,"probabilities":probabilities,"confidence":1.0}));
        }
        request
            .respond(tiny_http::Response::from_string(
                json!({"answers":answers,"usage":{"input_tokens":1000}}).to_string(),
            ))
            .unwrap();
    });
    let (_viewer, address, client) = start(Some(&api_endpoint));
    let endpoint = format!("{address}/api/parse");
    let parse_client = client.clone();
    let parse = thread::spawn(move || {
        parse_client
            .post(endpoint)
            .json(&json!({"text":"I see you."}))
            .send()
            .unwrap()
            .json::<Value>()
            .unwrap()
    });
    received.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(client.get(&address).send().unwrap().status().is_success());
    release.send(()).unwrap();
    let result = parse.join().unwrap();
    worker.join().unwrap();
    assert_eq!(result["view"]["tokens"], 4);
    assert_eq!(result["view"]["local"], 2);
    assert!(
        result["view"]["content"]
            .as_str()
            .unwrap()
            .contains("direct object")
    );
    assert!(
        result["cost"]
            .as_str()
            .unwrap()
            .contains("Tokens: 1000\nLuna Cost:")
    );
}

#[test]
fn saved_analysis_exports_without_a_live_parsing_form() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("analysis.json");
    std::fs::write(
        &file,
        json!({"sentences":[[{"text":"the","part_of_speech":"article","role":"determiner"}]]})
            .to_string(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_typesafe-ai"))
        .arg("parts-of-speech-serve")
        .arg(&file)
        .arg("--html")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success());
    let document = scraper::Html::parse_document(std::str::from_utf8(&output.stdout).unwrap());
    let form = document
        .select(&scraper::Selector::parse("#parse-form").unwrap())
        .next()
        .unwrap();
    assert!(form.value().attr("hidden").is_some());
    assert_eq!(
        document
            .select(&scraper::Selector::parse(".token").unwrap())
            .count(),
        1
    );
    let output = Command::new(env!("CARGO_BIN_EXE_typesafe-ai"))
        .args(["parts-of-speech-serve", "--html"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--html requires saved analysis JSON")
    );
}
