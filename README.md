# typesafe-ai

A Rust CLI built with [usage-rs](https://usage.jdx.dev/rust/) that evaluates text
using TypeSafe's System One API and returns a **Noul**: the probability that the
text contains personal health information (PHI).

## Install

```sh
cargo install --path . --locked
export TYPESAFE_API_KEY='your-api-key'
```

## Usage

```sh
typesafe-ai phi patient-notes.txt
typesafe-ai phi ./notes
cat patient-notes.txt | typesafe-ai phi
typesafe-ai phi - < patient-notes.txt
typesafe-ai phi patient-notes.txt --model jev-latest
typesafe-ai phi --help
```

The file argument accepts UTF-8 text, including Markdown, CSV, and JSON as text.
Input is sent in full to TypeSafe in one evaluation, without truncation. PDF and
other binary formats must first be converted to text. Empty input is an error.
When a file is supplied, it takes precedence over stdin.

Directories include hidden regular files, without recursing into subdirectories
or following symlinks. PHI evaluations launch concurrently for all files, with
each result printed and flushed immediately in completion order. Each file is
evaluated separately. A failed file is reported on stderr; scanning continues and the command
exits nonzero if any files failed. An empty directory produces no output.

Successful output is one line per file: the Noul value followed by its base name
(stdin uses `-`), for example:

```text
0.97 patient-notes.txt
0.08 general-advice.txt
```

Only the Noul value is colored: **red below 0.2**, **green at or above 0.2**.
Colors are enabled on a terminal and omitted when piping or redirecting output.
Set `CLICOLOR_FORCE=1` to force colors, or `NO_COLOR=1` to disable them.

`noul` is the probability from 0 to 1 that the input contains PHI. It is returned
directly from the model without rounding or thresholding. This command defines
PHI as health, healthcare, or healthcare-payment information linked to an
identified or reasonably identifiable person. General medical discussion,
de-identified statistics, and identifiers alone do not qualify.

## Code comment review

```sh
typesafe-ai code-comment example.ts
typesafe-ai code-comment ./src
cat example.ts | typesafe-ai code-comment
typesafe-ai code-comment - < example.ts
```

Uses a Tree-sitter JS/TS-compatible grammar to locate `//` and `/* ... */`
comments (including JSDoc and JSX comments), avoiding comment-like text inside
strings, regular expressions, and template literal text. Adjacent `//` lines
are grouped into one comment. Line spans are 1-based and inclusive.

Each file's full source and identified comments are sent in one TypeSafe request,
with **two independent Noul questions per comment**:

- **Accurate:** Does the comment accurately describe the following code? Inline
  comments also consider their associated code on the same line.
- **Necessary:** Does it add useful understanding, explain non-obvious code, or
  capture historical or human reasons?

Example output (bars are colored in terminals):

```text
example.ts:22-24
Accurate:  ██████████████████▉░░░░░ 79%
Necessary: █▉░░░░░░░░░░░░░░░░░░░░░░ 8%
// Line 22 of the comment
// Line 23 of the comment
// Line 24 of the comment

---
```

Bar colors interpolate continuously from red at 0%, through yellow at 50%, to
green at 100%. Percentages are rounded for display. `NO_COLOR` and
`CLICOLOR_FORCE` work as for `phi`. Files without comments produce no output
and require no API call. Directories scan immediate regular files in filename
order, continuing after per-file errors; nested directories and symlinks are
skipped. Stdin is labeled `-`.

Historical claims are evaluated against available source context, rather than
external history. Source is sent in full without truncation, so it must fit the
selected model's request limits.

## Configuration

| Environment variable | Purpose | Default |
| --- | --- | --- |
| `TYPESAFE_API_KEY` | Required API key | — |
| `TYPESAFE_MODEL` | Model; overridden by `--model` | `jev-latest` |
| `TYPESAFE_ENDPOINT` | Full evaluation endpoint URL | `https://api.typesafe.ai/v1/systemone` |

Errors go to stderr with a nonzero exit status (1 for input/API errors, 2 for
argument errors). Rate-limit and overload responses receive up to two retries
with exponential backoff. Each request has a 120-second timeout.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Tests use a local mock API and require no credentials.
