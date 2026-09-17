# Load-bearing example

`order-processing.ts` is a 108-line illustrative checkout workflow with validation,
inventory locking, payment idempotency, transaction boundaries, receipt delivery,
and routine logging. Its service interfaces are illustrative adapter contracts,
not a runnable payment system.

The accompanying JSONL contains **actual TypeSafe `jev-latest` scores**, generated
with the runtime-focused, enclosing-operation-aware rubric on 2026-09-17,
explicit Tree-sitter statement context, and a snapshot of the evaluated
source. Comment-only lines are omitted. Scores are probability-weighted judgments:
type-only declarations are instructed to score zero but can receive small nonzero
values from the model. From the repository root, preview it without an API key:

```sh
cargo run -- load-bearing-serve resources/load-bearing/order-processing.scores.jsonl
```

Open http://127.0.0.1:8230. To regenerate TypeSafe scores:

```sh
cargo run -- load-bearing resources/load-bearing/order-processing.ts > scores.jsonl
cargo run -- load-bearing-serve scores.jsonl
```
