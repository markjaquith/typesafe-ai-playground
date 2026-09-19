use std::sync::{Arc, Mutex};

use serde::Deserialize;

#[derive(Clone, Default)]
pub(crate) struct Cost(Arc<Mutex<Totals>>);

#[derive(Default)]
struct Totals {
    tokens: u128,
    missing_usage: usize,
    // Thousandths of 0.0001 cent, rounded only when displaying the total.
    luna_units: u128,
    missing_luna: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn totals_input_tokens_only_and_rounds_once() {
        let cost = Cost::default();
        assert_eq!(
            cost.summary(),
            "Cost: 0.0000¢\nTokens: 0\nLuna Cost: 0.0000¢"
        );
        cost.record(Some(
            &json!({"answers": {}, "usage": {"input_tokens": 6, "output_tokens": 1_000_000}}),
        ));
        cost.clone()
            .record(Some(&json!({"answers": {}, "usage": {"input_tokens": 6}})));
        assert_eq!(
            cost.summary(),
            "Cost: 0.0001¢\nTokens: 12\nLuna Cost: 0.0005¢ (9.5× Jev)"
        );
        let million = Cost::default();
        million.record(Some(
            &json!({"answers": {}, "usage": {"input_tokens": 1_000_000, "output_tokens": 500_000}}),
        ));
        assert_eq!(
            million.summary(),
            "Cost: 4.2000¢\nTokens: 1000000\nLuna Cost: 40.0002¢ (9.5× Jev)"
        );
    }

    #[test]
    fn missing_usage_is_not_misreported_as_zero_cost() {
        let cost = Cost::default();
        cost.record(Some(&json!({"usage": {"input_tokens": 1000}})));
        cost.record(None);
        cost.record(Some(&json!({"usage": {"output_tokens": 10}})));
        assert_eq!(
            cost.summary(),
            "Cost: unavailable (known cost: 0.0042¢; token usage missing for 2 request(s))\n\
             Tokens: unavailable (known tokens: 1000; usage missing for 2 request(s))\n\
             Luna Cost: unavailable (known cost: 0.0000¢; input usage or answers missing for 3 request(s))"
        );
    }

    #[test]
    fn luna_prices_each_request_before_merging() {
        let cost = Cost::default();
        let other = Cost::default();
        cost.record(Some(
            &json!({"answers": {}, "usage": {"input_tokens": 272_000}}),
        ));
        other.record(Some(
            &json!({"answers": {}, "usage": {"input_tokens": 272_001}}),
        ));
        cost.merge(&other);
        assert!(cost.summary().ends_with("Luna Cost: 16.3203¢ (7.1× Jev)"));
    }
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: u64,
}

impl Cost {
    pub(crate) fn merge(&self, other: &Self) {
        let other = other.0.lock().unwrap();
        let mut totals = self.0.lock().unwrap();
        totals.tokens += other.tokens;
        totals.missing_usage += other.missing_usage;
        totals.luna_units += other.luna_units;
        totals.missing_luna += other.missing_luna;
    }

    pub(crate) fn record(&self, response: Option<&serde_json::Value>) {
        let usage = response
            .and_then(|response| response.get("usage"))
            .and_then(|usage| serde_json::from_value::<Usage>(usage.clone()).ok());
        // Count just the model's answers, not transport metadata or usage fields.
        // encode_ordinary treats special-token-looking answer text as literal text.
        let output_tokens = response
            .and_then(|response| response.get("answers"))
            .filter(|answers| answers.is_object())
            .map(|answers| {
                tiktoken_rs::o200k_base_singleton()
                    .encode_ordinary(&answers.to_string())
                    .len() as u128
            });
        let mut totals = self.0.lock().unwrap();
        if let (Some(usage), Some(output_tokens)) = (&usage, output_tokens) {
            // OpenAI standard GPT-5.6 Luna pricing, checked 2026-09-19:
            // https://developers.openai.com/api/docs/models/gpt-5.6-luna
            // $0.20/$1.20 per million input/output; >272K: $0.40/$1.80.
            let (input_rate, output_rate) = if usage.input_tokens > 272_000 {
                (400, 1800)
            } else {
                (200, 1200)
            };
            totals.luna_units +=
                u128::from(usage.input_tokens) * input_rate + output_tokens * output_rate;
        } else {
            totals.missing_luna += 1;
        }
        if let Some(usage) = usage {
            totals.tokens += u128::from(usage.input_tokens);
        } else {
            totals.missing_usage += 1;
        }
    }

    pub(crate) fn summary(&self) -> String {
        let totals = self.0.lock().unwrap();
        // $0.042 / million input tokens = 4.2 US pennies / million input tokens.
        // Round once, after aggregation, to units of 0.0001 penny.
        let units = (totals.tokens * 42 + 500) / 1000;
        let pennies = format!("{}.{:04}", units / 10_000, units % 10_000);
        let jev = if totals.missing_usage == 0 {
            format!("Cost: {pennies}¢\nTokens: {}", totals.tokens)
        } else {
            format!(
                "Cost: unavailable (known cost: {pennies}¢; token usage missing for {missing} request(s))\n\
                 Tokens: unavailable (known tokens: {tokens}; usage missing for {missing} request(s))",
                missing = totals.missing_usage,
                tokens = totals.tokens,
            )
        };
        let units = (totals.luna_units + 500) / 1000;
        let pennies = format!("{}.{:04}", units / 10_000, units % 10_000);
        let luna = if totals.missing_luna == 0 {
            let multiplier = if totals.missing_usage == 0 && totals.tokens > 0 {
                format!(
                    " ({:.1}× Jev)",
                    totals.luna_units as f64 / (totals.tokens * 42) as f64
                )
            } else {
                String::new()
            };
            format!("Luna Cost: {pennies}¢{multiplier}")
        } else {
            format!(
                "Luna Cost: unavailable (known cost: {pennies}¢; input usage or answers missing for {} request(s))",
                totals.missing_luna,
            )
        };
        format!("{jev}\n{luna}")
    }
}
