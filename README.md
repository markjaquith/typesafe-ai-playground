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
typesafe-ai code-comments example.ts
typesafe-ai code-comments ./src
cat example.ts | typesafe-ai code-comments
typesafe-ai code-comments - < example.ts
```

Uses a Tree-sitter JS/TS-compatible grammar to locate `//` and `/* ... */`
comments (including JSDoc and JSX comments), avoiding comment-like text inside
strings, regular expressions, and template literal text. Adjacent `//` lines
are grouped into one comment. Line spans are 1-based and inclusive.

Each file's full source and identified comments are sent in one TypeSafe request,
with **two independent Score questions per comment**:

- **Accuracy:** How accurately does the comment describe the following code? Inline
  comments also consider their associated code on the same line.
- **Usefulness:** How much does it add useful understanding, explain non-obvious code, or
  capture historical or human reasons?

Example output (bars are colored in terminals):

```text
example.ts:22-24
Accuracy:   ███████████████████      79/100 · Mostly accurate
Usefulness: █▉                       8/100 · No useful information
╭──────────────────────────╮
│ // Line 22 of the comment │
│ // Line 23 of the comment │
│ // Line 24 of the comment │
│ const value = compute(); │
╰──────────────────────────╯
```

Each box includes the next physical source line after the comment (when present),
with JS/TS syntax highlighting in terminals. Comment indentation is normalized,
retaining one space before block-comment `*` lines. The header's line span refers
only to the comment itself.

Each axis uses four rubric levels (0–3):

| Level | Accuracy | Usefulness |
| --- | --- | --- |
| 0 | Fundamentally wrong or contradicts the code | No useful information |
| 1 | Substantial errors alongside correct information | Slight orientation, mostly redundant |
| 2 | Mostly accurate with minor misleading details | Explains non-obvious behavior, intent, or constraints |
| 3 | Accurate without material errors | Important rationale or context not readily inferable from code |

The API's probability-weighted Score is divided by 3 and displayed as a rounded
rating out of 100, not a probability or confidence.

Each rating also shows the nearest rubric level and a short description, rounding
the original 0–3 Score to the closest integer (halfway values round upward).
This is a summary of the weighted Score, not the most probable individual level.

Bar colors interpolate
continuously from red at 0, through yellow at 50, to green at 100. `NO_COLOR` and
`CLICOLOR_FORCE` work as for `phi`. Files without comments produce no output
and require no API call. Directories scan immediate regular files in filename
order, continuing after per-file errors; nested directories and symlinks are
skipped. Stdin is labeled `-`.

Historical claims are evaluated against available source context, rather than
external history. Source is sent in full without truncation, so it must fit the
selected model's request limits.

## Live tone analysis

```sh
typesafe-ai be-nice
typesafe-ai be-nice --model jev-latest
```

Opens an interactive terminal view with a single-line text editor and a five-level
tone scale:

** Really nice →  Kinda nice → 󰇶 Neutral →  Kinda mean → 󰱪 Mean**

After a **50ms pause in typing**, the current text is sent as one Score question.
Its weighted 0–4 score positions the icon on a green-to-red bar. Old responses
cannot overwrite newer text's score. The last score remains visible while editing;
empty input clears the marker and score text and is not scored. Cost is reported
on exit and counts completed requests, including superseded ones.
Nerd Font icons render best with a Nerd Font terminal font; text labels are also
shown.

Use arrow keys or **Ctrl-B/F** to move, **Home/End** or **Ctrl-A/E** for start/end,
**Alt-B/F** to move by word, **Ctrl-U/K/W** to cut, and **Ctrl-Y** to yank.
**Backspace/Ctrl-H** deletes left; **Delete/Ctrl-D** deletes right. Pasted newlines
are converted to spaces. **F1** shows editing help, **Esc** closes help, and
**Ctrl-C** quits. Ordinary `q`, `j`, and `?` remain text input.

Exiting restores the terminal and prints the cost summary to stderr. If requests
are still in flight on exit, their costs are reported as unknown rather than
blocking terminal exit. `NO_COLOR` disables the color scale.

## Configuration

All commands print one cost summary to **stderr** when finished, for example:

```text
Cost: 0.0042¢
```

Cost is calculated from the API's `usage.input_tokens` at **$0.042 per million
input tokens** (4.2 US pennies per million). Output tokens are free. Input usage
is summed across all requests, including concurrent files and any retry responses
that report usage, then rounded once to four decimal places. A batched comment
request is counted once, not once per question. No queries means `Cost: 0.0000¢`.

The documented API provides token counts rather than a monetary cost field.
If a request's usage is missing (including a failed request without usage), the
summary reports the total as unavailable and shows the known cost separately.
Normal results remain on stdout.

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
