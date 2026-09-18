use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use crate::cost::Cost;

#[derive(Deserialize)]
struct Response {
    answers: HashMap<String, Answer>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum Answer {
    Noul {
        noul: f64,
    },
    Score {
        score: f64,
    },
    Choice {
        choice: String,
        probabilities: HashMap<String, f64>,
        confidence: f64,
    },
}

#[derive(Clone)]
pub(crate) struct Client {
    http: reqwest::blocking::Client,
    endpoint: String,
    key: String,
    provider: Provider,
}

#[derive(Clone, Copy)]
enum Provider {
    TypeSafe,
    OpenRouter,
}

impl Client {
    pub(crate) fn from_env() -> Result<Self> {
        let (provider, key, endpoint) = match std::env::var("OPENROUTER_API_KEY") {
            Ok(key) => (
                Provider::OpenRouter,
                key,
                std::env::var("OPENROUTER_ENDPOINT")
                    .unwrap_or_else(|_| "https://openrouter.ai/api/alpha/decisions".into()),
            ),
            Err(std::env::VarError::NotPresent) => (
                Provider::TypeSafe,
                std::env::var("TYPESAFE_API_KEY")
                    .context("set OPENROUTER_API_KEY or TYPESAFE_API_KEY to an API key")?,
                std::env::var("TYPESAFE_ENDPOINT")
                    .unwrap_or_else(|_| "https://api.typesafe.ai/v1/systemone".into()),
            ),
            Err(error) => return Err(error).context("could not read OPENROUTER_API_KEY"),
        };
        let key_name = match provider {
            Provider::TypeSafe => "TYPESAFE_API_KEY",
            Provider::OpenRouter => "OPENROUTER_API_KEY",
        };
        ensure!(!key.trim().is_empty(), "{key_name} is empty");
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("could not create HTTP client")?;
        Ok(Self {
            http,
            endpoint,
            key,
            provider,
        })
    }

    pub(crate) fn request(
        &self,
        body: &serde_json::Value,
        cost: &Cost,
    ) -> Result<HashMap<String, Answer>> {
        let mut body = body.clone();
        if matches!(self.provider, Provider::OpenRouter)
            && let Some(model) = body["model"].as_str()
        {
            body["model"] = openrouter_model(model).into();
        }
        for attempt in 0..3 {
            let response = self
                .http
                .post(&self.endpoint)
                .bearer_auth(&self.key)
                .json(&body)
                .send()
                .inspect_err(|_| cost.record(None))
                .context("TypeSafe request failed")?;
            let status = response.status();
            let payload = response.json::<serde_json::Value>();
            cost.record(payload.as_ref().ok());
            if matches!(status.as_u16(), 429 | 529) && attempt < 2 {
                std::thread::sleep(Duration::from_secs(1 << attempt));
                continue;
            }
            if !status.is_success() {
                let hint = match status.as_u16() {
                    401 | 403 => match self.provider {
                        Provider::TypeSafe => "check TYPESAFE_API_KEY",
                        Provider::OpenRouter => "check OPENROUTER_API_KEY",
                    },
                    422 => "check the model and input size against the API limits",
                    429 | 529 => "service busy; try again later",
                    _ => "request was unsuccessful",
                };
                bail!("TypeSafe returned HTTP {status}: {hint}");
            }
            let result: Response =
                serde_json::from_value(payload.context("invalid TypeSafe response")?)
                    .context("invalid TypeSafe answer response")?;
            validate_answers(&body, &result.answers)?;
            return Ok(result.answers);
        }
        unreachable!("the final attempt returns a result")
    }
}

fn openrouter_model(model: &str) -> String {
    match model {
        "jev-latest" => "typesafe/jev-1.13".into(),
        model if model.starts_with("typesafe/") || model.starts_with("~typesafe/") => model.into(),
        model => format!("typesafe/{model}"),
    }
}

fn validate_answers(body: &serde_json::Value, answers: &HashMap<String, Answer>) -> Result<()> {
    for (id, answer) in answers {
        let question = &body["questions"][id];
        let (value, maximum) = match answer {
            Answer::Choice {
                choice,
                probabilities,
                confidence,
            } => {
                ensure!(
                    question["type"] == "choice",
                    "unexpected Choice answer for {id}"
                );
                let options = question["criteria"]
                    .as_object()
                    .context("missing Choice criteria")?;
                ensure!(
                    options.contains_key(choice),
                    "unknown Choice option for {id}"
                );
                ensure!(
                    probabilities.len() == options.len()
                        && probabilities.keys().all(|key| options.contains_key(key)),
                    "Choice probabilities do not match options for {id}"
                );
                ensure!(
                    probabilities
                        .values()
                        .all(|p| p.is_finite() && (0.0..=1.0).contains(p)),
                    "invalid Choice probabilities for {id}"
                );
                ensure!(
                    (probabilities.values().sum::<f64>() - 1.0).abs() < 0.02,
                    "Choice probabilities do not sum to one for {id}"
                );
                ensure!(
                    probabilities
                        .values()
                        .all(|p| *p <= probabilities[choice] + 1e-6),
                    "Choice winner does not match probabilities for {id}"
                );
                (*confidence, 1.0)
            }
            Answer::Noul { noul } => {
                ensure!(
                    question["type"] == "noul",
                    "unexpected Noul answer for {id}"
                );
                (*noul, 1.0)
            }
            Answer::Score { score } => {
                ensure!(
                    question["type"] == "score",
                    "unexpected Score answer for {id}"
                );
                let levels = question["criteria"]
                    .as_array()
                    .context("missing Score criteria")?;
                ensure!(levels.len() >= 2, "Score needs at least two levels");
                (*score, (levels.len() - 1) as f64)
            }
        };
        ensure!(
            value.is_finite() && (0.0..=maximum).contains(&value),
            "TypeSafe returned an answer outside the range 0..={maximum} for {id}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_typesafe_model_names_for_openrouter() {
        assert_eq!(openrouter_model("jev-latest"), "typesafe/jev-1.13");
        assert_eq!(openrouter_model("jev-1.13"), "typesafe/jev-1.13");
        assert_eq!(openrouter_model("typesafe/jev-1.13"), "typesafe/jev-1.13");
    }

    #[test]
    fn choice_response_cannot_invent_codes_or_probabilities() {
        for (answer, valid) in [
            (
                json!({"type": "choice", "choice": "known", "probabilities": {"known": 0.9, "other": 0.1}, "confidence": 0.8}),
                true,
            ),
            (
                json!({"type": "choice", "choice": "invented", "probabilities": {"known": 0.9, "other": 0.1}, "confidence": 0.8}),
                false,
            ),
            (
                json!({"type": "choice", "choice": "known", "probabilities": {"known": 1.0, "invented": 0.0}, "confidence": 0.8}),
                false,
            ),
            (
                json!({"type": "choice", "choice": "known", "probabilities": {"known": 1.2, "other": -0.2}, "confidence": 0.8}),
                false,
            ),
            (
                json!({"type": "choice", "choice": "known", "probabilities": {"known": 0.1, "other": 0.1}, "confidence": 0.8}),
                false,
            ),
        ] {
            let answers: Response = serde_json::from_value(json!({
                "answers": {"classification": answer}
            }))
            .unwrap();
            let result = validate_answers(
                &json!({"questions": {"classification": {"type": "choice", "criteria": {"known": "Known", "other": "Other"}}}}),
                &answers.answers,
            );
            assert_eq!(result.is_ok(), valid);
        }
    }
}
