use std::{
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use usage::Args;

use crate::{Cost, parts_of_speech, read_input};

#[derive(Args)]
#[usage(unknown_flags = "error")]
pub struct Options {
    /// Saved JSON file or - for stdin; omit for an interactive demo (or pipe JSON).
    file: Option<PathBuf>,
    /// Address on which to serve the viewer.
    #[usage(long, default = "127.0.0.1:8231")]
    bind: String,
    /// Write self-contained HTML to stdout instead of starting a server.
    #[usage(long)]
    html: bool,
    /// TypeSafe model for interactive parsing.
    #[usage(long, env = "TYPESAFE_MODEL", default = "jev-latest")]
    model: String,
}

#[derive(Deserialize)]
struct Analysis {
    sentences: Vec<Vec<Token>>,
}

#[derive(Deserialize)]
struct Token {
    text: String,
    part_of_speech: String,
    role: Option<String>,
    confidence: Option<Confidence>,
}

#[derive(Deserialize)]
struct Confidence {
    part_of_speech: Option<f64>,
    role: Option<f64>,
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn family(label: &str) -> (&'static str, &'static str) {
    let label = label.split(" (").next().unwrap_or(label);
    if label.contains("pronoun") {
        ("pronoun", "Pronoun")
    } else if label.contains("determiner") || label.starts_with("article") {
        ("determiner", "Determiner")
    } else if label.contains("adjective") && !label.starts_with("verb,") {
        ("adjective", "Adjective")
    } else if label.contains("adverb") {
        ("adverb", "Adverb")
    } else if label.contains("noun") || label.starts_with("gerund") {
        ("noun", "Noun / gerund")
    } else if label.contains("verb") || label.starts_with("infinitival") {
        ("verb", "Verb")
    } else if label.contains("conjunction")
        || label.contains("preposition")
        || label.contains("particle")
    {
        ("connector", "Connector")
    } else {
        ("other", "Other / punctuation")
    }
}

fn role_family(label: Option<&str>) -> (&'static str, &'static str) {
    match label {
        Some(label) if label.starts_with("subject (head") => ("noun", "Subject"),
        Some(label)
            if label.starts_with("direct object")
                || label.starts_with("indirect object")
                || label.starts_with("object of") =>
        {
            ("pronoun", "Object")
        }
        Some(label) if label.contains("complement") => ("adjective", "Complement"),
        Some(label) if label.contains("modifier") => ("adverb", "Modifier"),
        Some(label) if label.contains("verb") || label.contains("auxiliary") => {
            ("verb", "Predicate / auxiliary")
        }
        Some(label) if label.contains("determiner") => ("determiner", "Determiner"),
        Some(label) if label.contains("connector") => ("connector", "Connector"),
        _ => ("other", "Other / punctuation"),
    }
}

fn confidence(value: Option<f64>, applicable: bool) -> String {
    match value {
        Some(value) => format!("Jev · {:.0}% confidence", value * 100.0),
        None if applicable => "Local rule · no model confidence".into(),
        None => "Not applicable".into(),
    }
}

#[derive(Default, Serialize)]
struct Rendered {
    content: String,
    sentences: usize,
    tokens: usize,
    local: usize,
    uncertain: usize,
}

fn render_content(input: &str) -> Result<Rendered> {
    let analysis: Analysis = serde_json::from_str(input).context("invalid parts-of-speech JSON")?;
    ensure!(
        !analysis.sentences.is_empty(),
        "analysis contains no sentences"
    );
    let mut content = String::new();
    let mut count = 0;
    let mut local = 0;
    let mut uncertain = 0;
    for (s, sentence) in analysis.sentences.iter().enumerate() {
        ensure!(
            !sentence.is_empty(),
            "sentence {} contains no tokens",
            s + 1
        );
        content.push_str(&format!("<section class=sentence aria-labelledby=s{s}><header><h2 id=s{s}><span>Sentence</span> {:02}</h2><span>{} tokens</span></header><div class=tokens>", s + 1, sentence.len()));
        for (w, token) in sentence.iter().enumerate() {
            ensure!(
                !token.text.is_empty() && !token.part_of_speech.is_empty(),
                "empty token or label at sentence {}, token {}",
                s + 1,
                w + 1
            );
            let pc = token.confidence.as_ref().and_then(|c| c.part_of_speech);
            let rc = token.confidence.as_ref().and_then(|c| c.role);
            for value in [pc, rc].into_iter().flatten() {
                ensure!(
                    value.is_finite() && (0.0..=1.0).contains(&value),
                    "confidence must be between 0 and 1 at sentence {}, token {}",
                    s + 1,
                    w + 1
                );
            }
            let low = [pc, rc].into_iter().flatten().any(|c| c < 0.6);
            count += 1;
            local += usize::from(pc.is_none() && rc.is_none());
            uncertain += usize::from(low);
            let (part_family, part_label) = family(&token.part_of_speech);
            let (role_family, role_label) = role_family(token.role.as_deref());
            let label = token.part_of_speech.split(" (").next().unwrap();
            let role = token.role.as_deref().unwrap_or("No grammatical role");
            let short_role = role.split(" (").next().unwrap();
            let text = escape(&token.text);
            content.push_str(&format!(
                "<button type=button class=token data-family=\"{part_family}\" data-part-family=\"{part_family}\" data-role-family=\"{role_family}\" data-part-group=\"{part_label}\" data-role-group=\"{role_label}\" data-low=\"{low}\" data-position=\"Sentence {}, token {} · [{}][{}]\" data-part=\"{}\" data-role=\"{}\" data-pc=\"{}\" data-rc=\"{}\" aria-describedby=tooltip><span class=word>{text}</span><span class=part-label>{}</span><span class=role-label>{}</span><span class=uncertainty aria-label=\"Low model confidence\">?</span></button>",
                s+1, w+1, s, w, escape(&token.part_of_speech), escape(role), confidence(pc, true), confidence(rc, token.role.is_some()), escape(label), escape(short_role)
            ));
        }
        content.push_str("</div></section>");
    }
    Ok(Rendered {
        content,
        sentences: analysis.sentences.len(),
        tokens: count,
        local,
        uncertain,
    })
}

fn page(view: &Rendered, interactive: bool) -> String {
    // Replace content last so user text cannot be interpreted as template markers.
    include_str!("parts_of_speech.html")
        .replace("<!--SENTENCES-->", &view.sentences.to_string())
        .replace("<!--TOKENS-->", &view.tokens.to_string())
        .replace("<!--LOCAL-->", &view.local.to_string())
        .replace("<!--UNCERTAIN-->", &view.uncertain.to_string())
        .replace(
            "<!--FORM-HIDDEN-->",
            if interactive { "" } else { "hidden" },
        )
        .replace(
            "<!--EMPTY-HIDDEN-->",
            if view.tokens == 0 { "" } else { "hidden" },
        )
        .replace("<!--CONTENT-->", &view.content)
}

#[cfg(test)]
fn render(input: &str) -> Result<String> {
    Ok(page(&render_content(input)?, false))
}

#[derive(Deserialize)]
struct ParseInput {
    text: String,
}

fn parse_response(request: &mut tiny_http::Request, model: &str) -> (u16, serde_json::Value) {
    if !request.headers().iter().any(|header| {
        header.field.equiv("Content-Type")
            && header
                .value
                .as_str()
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    }) {
        return (
            415,
            json!({"error": "send application/json with a text field"}),
        );
    }
    let mut bytes = Vec::new();
    const MAX_BODY: u64 = 1_048_576;
    if let Err(error) = request
        .as_reader()
        .take(MAX_BODY + 1)
        .read_to_end(&mut bytes)
    {
        return (
            400,
            json!({"error": format!("could not read request: {error}")}),
        );
    }
    if bytes.len() as u64 > MAX_BODY {
        return (413, json!({"error": "input exceeds 1 MiB request limit"}));
    }
    let input = match serde_json::from_slice::<ParseInput>(&bytes) {
        Ok(input) => input,
        Err(error) => {
            return (
                400,
                json!({"error": format!("invalid parse request: {error}")}),
            );
        }
    };
    if input.text.trim().is_empty() || input.text.contains('\0') {
        return (
            400,
            json!({"error": "enter nonempty text without NUL bytes"}),
        );
    }
    let cost = Cost::default();
    let result = parts_of_speech::analyze(&input.text, model, &cost)
        .and_then(|analysis| render_content(&analysis.to_string()));
    match result {
        Ok(view) => (200, json!({"view": view, "cost": cost.summary()})),
        Err(error) => (
            502,
            json!({"error": format!("{error:#}"), "cost": cost.summary()}),
        ),
    }
}

pub fn run(args: Options) -> Result<()> {
    let input = if args.file.is_some() {
        Some(read_input(args.file.as_deref())?)
    } else if io::stdin().is_terminal() {
        None
    } else {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .context("could not read stdin")?;
        (!input.trim().is_empty()).then_some(input)
    };
    ensure!(
        !args.html || input.is_some(),
        "--html requires saved analysis JSON from a file or stdin"
    );
    let view = input
        .as_deref()
        .map(render_content)
        .transpose()?
        .unwrap_or_default();
    let html = page(&view, !args.html);
    if args.html {
        io::stdout().lock().write_all(html.as_bytes())?;
        return Ok(());
    }
    let server = tiny_http::Server::http(&args.bind)
        .map_err(|error| anyhow::anyhow!("could not bind server: {error}"))?;
    eprintln!(
        "Parts-of-speech viewer: http://{} (Ctrl-C to stop)",
        server.server_addr()
    );
    for request in server.incoming_requests() {
        if request.url() == "/api/parse" && *request.method() == tiny_http::Method::Post {
            let model = args.model.clone();
            // Keep serving the page while a blocking Jev request is in flight.
            std::thread::spawn(move || {
                let mut request = request;
                let (status, body) = parse_response(&mut request, &model);
                let response = tiny_http::Response::from_string(body.to_string())
                    .with_status_code(status)
                    .with_header(
                        tiny_http::Header::from_bytes(
                            "Content-Type",
                            "application/json; charset=utf-8",
                        )
                        .unwrap(),
                    );
                if let Err(error) = request.respond(response) {
                    eprintln!("HTTP response error: {error}");
                }
            });
            continue;
        }
        let response = if request.url() == "/"
            && matches!(
                request.method(),
                tiny_http::Method::Get | tiny_http::Method::Head
            ) {
            tiny_http::Response::from_string(html.as_str()).with_header(
                tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap(),
            )
        } else {
            tiny_http::Response::from_string("Not found").with_status_code(404)
        };
        if let Err(error) = request.respond(response) {
            eprintln!("HTTP response error: {error}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_literal_tokens_labels_and_mixed_confidence_without_html_injection() {
        let input = json!({"sentences": [[
            {"text": "<script>alert('x')</script>", "part_of_speech": "common noun, singular or mass", "role": "subject (head)", "confidence": {"part_of_speech": null, "role": 0.4}},
            {"text": "!", "part_of_speech": "exclamation mark", "role": null}
        ]]}).to_string();
        let html = render(&input).unwrap();
        let document = scraper::Html::parse_document(&html);
        let selector = scraper::Selector::parse(".token").unwrap();
        let tokens: Vec<_> = document.select(&selector).collect();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].value().attr("data-low"), Some("true"));
        assert_eq!(
            tokens[0].value().attr("data-pc"),
            Some("Local rule · no model confidence")
        );
        assert_eq!(
            tokens[0].value().attr("data-rc"),
            Some("Jev · 40% confidence")
        );
        let word = scraper::Selector::parse(".word").unwrap();
        assert_eq!(
            tokens[0]
                .select(&word)
                .next()
                .unwrap()
                .text()
                .collect::<String>(),
            "<script>alert('x')</script>"
        );
        assert!(!html.contains("<script>alert('x')</script>"));
        assert_eq!(tokens[1].value().attr("data-rc"), Some("Not applicable"));
    }

    #[test]
    fn rejects_empty_analysis_and_invalid_confidence() {
        for input in [
            json!({"sentences": []}),
            json!({"sentences": [[]]}),
            json!({"sentences": [[{"text": "word", "part_of_speech": "noun", "confidence": {"role": 1.1}}]]}),
        ] {
            assert!(render(&input.to_string()).is_err());
        }
        assert_eq!(
            family("verb, present participle (verbal -ing, not a noun or adjective)").0,
            "verb"
        );
        assert_eq!(role_family(Some("adverbial modifier")).1, "Modifier");
    }
}
