use std::sync::{Arc, Mutex};

use serde::Deserialize;

#[derive(Clone, Default)]
pub(crate) struct Cost(Arc<Mutex<Totals>>);

#[derive(Default)]
struct Totals {
    tokens: u128,
    missing_usage: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn totals_input_tokens_only_and_rounds_once() {
        let cost = Cost::default();
        assert_eq!(cost.summary(), "Cost: 0.0000¢\nTokens: 0");
        cost.record(Some(
            &json!({"usage": {"input_tokens": 6, "output_tokens": 1_000_000}}),
        ));
        cost.clone()
            .record(Some(&json!({"usage": {"input_tokens": 6}})));
        assert_eq!(cost.summary(), "Cost: 0.0001¢\nTokens: 12");
        let million = Cost::default();
        million.record(Some(
            &json!({"usage": {"input_tokens": 1_000_000, "output_tokens": 500_000}}),
        ));
        assert_eq!(million.summary(), "Cost: 4.2000¢\nTokens: 1000000");
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
             Tokens: unavailable (known tokens: 1000; usage missing for 2 request(s))"
        );
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
    }

    pub(crate) fn record(&self, response: Option<&serde_json::Value>) {
        let usage = response
            .and_then(|response| response.get("usage"))
            .and_then(|usage| serde_json::from_value::<Usage>(usage.clone()).ok());
        let mut totals = self.0.lock().unwrap();
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
        if totals.missing_usage == 0 {
            format!("Cost: {pennies}¢\nTokens: {}", totals.tokens)
        } else {
            format!(
                "Cost: unavailable (known cost: {pennies}¢; token usage missing for {missing} request(s))\n\
                 Tokens: unavailable (known tokens: {tokens}; usage missing for {missing} request(s))",
                missing = totals.missing_usage,
                tokens = totals.tokens,
            )
        }
    }
}
