# TypeSafe AI Playground

Just a playground for experiments around [Jev](https://docs.typesafe.ai/),
TypeSafe's System One model.

The experiments live in a Rust CLI called `typesafe-ai`, built with
[usage-rs](https://usage.jdx.dev/rust/): PHI detection, code-comment review,
parts-of-speech and load-bearing analysis, interactive tone analysis, and
business and occupation classification.

## Install

```sh
cargo install --path . --locked
export OPENROUTER_API_KEY='your-openrouter-api-key'
```

Alternatively, set `TYPESAFE_API_KEY` to call TypeSafe directly. If both keys
are set, `OPENROUTER_API_KEY` takes precedence.

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

Successful output is four independently judged Noul metrics per file. Each line
contains the probability and metric. File scans append the base name; standard
input has no filename suffix:

```text
0.98 identifying information patient-notes.txt
0.96 health condition patient-notes.txt
0.84 healthcare provision patient-notes.txt
0.03 healthcare payment patient-notes.txt
```

The metrics mirror the independent elements in 45 CFR § 160.103:

- **Individually identifying information:** the text identifies a natural person
  or provides a reasonable basis to identify them. Its rubric covers HIPAA's
  direct and indirect Safe Harbor identifier categories, combinations of details,
  and identifiers of relatives, household members, and employers.
- **Health condition:** the text actually relates to an individual's past,
  present, or future physical or mental health or condition.
- **Healthcare provision:** the text actually relates to health care sought,
  offered, scheduled, provided, declined, or expected for an individual.
- **Healthcare payment:** the text actually relates to past, present, or future
  payment for health care provided or to be provided to an individual.

Each health nexus is judged separately from identifiability. General medical
education, aggregate statistics, provider listings, and a person's visit to a
health-related public webpage do not establish that the topic relates to that
person's own health or care. This follows the conjunctive reading in *American
Hospital Association v. Becerra*, No. 4:23-cv-01110-P (N.D. Tex. June 20,
2024): identifiability and at least one actual health nexus must both be present;
unstated subjective intent is not enough.

The command does not combine the metrics into a legal conclusion. HIPAA PHI
status also depends on facts text alone may not establish, including who created
or received the information, whether the holder is a covered entity or business
associate, regulatory exclusions, and whether a valid Safe Harbor or Expert
Determination de-identification process was completed. Treat these scores as
screening signals, not legal advice.

Rubrics are based on [45 CFR § 160.103](https://www.law.cornell.edu/cfr/text/45/160.103),
[45 CFR § 164.514](https://www.law.cornell.edu/cfr/text/45/164.514), HHS's
[de-identification guidance](https://www.hhs.gov/hipaa/for-professionals/special-topics/de-identification/index.html),
and the [2013 HIPAA omnibus rule compilation](https://www.hhs.gov/sites/default/files/ocr/privacy/hipaa/administrative/combined/hipaa-simplification-201303.pdf).

Only the Noul value is colored: **red below 0.2**, **green at or above 0.2**.
Colors are enabled on a terminal and omitted when piping or redirecting output.
Set `CLICOLOR_FORCE=1` to force colors, or `NO_COLOR=1` to disable them.

## Parts of speech

```sh
typesafe-ai parts-of-speech passage.txt --cost
echo 'The sleepy cat is chasing birds, happily!' | typesafe-ai parts-of-speech
typesafe-ai parts-of-speech - < passage.txt
typesafe-ai parts-of-speech passage.txt --request | jq .
```

Analyzes **English word types and grammatical roles**, emitting one JSON object
with nested `sentences` arrays. Accepts a UTF-8 file or stdin, `--model`, and the
global `--cost` flag. `--request` prints the exact compact request JSON without
calling an API or requiring credentials.

The input text occurs **once**, as arrays of tokens in shared state:

```json
[["The","sleepy","cat","is","chasing","birds",",","happily","!"]]
```

Each word has two independent classifications, with unresolved ones sent as
Choice questions in **one batched request**:

- **`p` — word type:** 47 options covering common/proper and singular/plural nouns,
  verb forms, participles, gerunds, auxiliaries, modals, copulas, imperatives,
  adjective/adverb degrees, pronoun and determiner varieties, conjunctions,
  prepositions, particles, numerals, interjections, contractions, and an other option.
- **`r` — role:** 17 options including subject, direct/indirect/prepositional object,
  complements, predicate verb, auxiliary, noun/adverbial modifiers, determiner,
  connector, appositive, vocative, discourse element, dummy subject, and other.

Question wording and option definitions live once in shared state. Individual
instructions are just `p: sentences[0][2]` or `r: sentences[0][2]` (the actual
request surrounds the path with backticks). Indices are **zero-based** and include
punctuation. Jev requires each question's own Choice option map, so short mnemonic
codes such as `NN`, `VBG`, and `SUBJ` repeat with `null` descriptions; the full
definitions and target text do not repeat. The shared-definition convention is
interpreted by the model, not a native API reference feature.

Common function words are classified locally in their ordinary English usage:
articles (`the`, `a`, `an`), conjunctions (`and`, `or`, `nor`), subject pronouns
(`I`, `he`, `she`, `we`, `they`), and possessive determiners (`my`, `your`, `our`,
`their`, `its`) need neither question. Other personal pronouns (`you`, `it`, `me`,
`him`, `us`, `them`) and reflexive pronouns have a known word type but still need
the role question. Ambiguous words such as `that`, `her`, `his`, `can`, and `to`
keep both questions. Matching allows sentence capitalization; uppercase acronyms
such as `US` and `IT` are left to Jev. These shortcuts assume ordinary usage,
not quoted words being discussed as nouns or unusual names.

Locally resolved words remain in the shared text at their original indices.
Unused question definitions are omitted. Inputs that resolve entirely locally
need no credentials and incur no API cost.

Output tokens contain `text`, `part_of_speech`, `role`, and separate confidence
values for the two judgments (`null` for any locally resolved axis, rather than
an invented model confidence). For example, the token `cat` might be returned as:

```json
{
  "text": "cat",
  "part_of_speech": "common noun, singular or mass",
  "role": "subject (head of a subject phrase)",
  "confidence": {"part_of_speech": 0.99, "role": 0.98}
}
```

Commas, periods, ellipses (`...` and `…`), exclamation/question marks, and other
punctuation remain separate array members. They are labeled locally, with a null
role and no model confidence, saving questions and tokens. Punctuation-only input
requires no credentials. All model answers are checked before any result is emitted.

Tokenization uses Unicode sentence/word boundaries, preserving spelling, case,
contractions, and decimal numbers; consecutive periods form one ellipsis token.
Whitespace is omitted. Sentence boundaries are heuristic (not a full grammatical
parser), and abbreviations or unusual punctuation can affect segmentation.
Contractions are classified as combined tokens rather than expanded into repeated
text. Model labels are judgments, not guaranteed parses; confidence is included
for ambiguous analyses. The entire input and question batch must fit the selected
model's request limits.

### Sentence viewer

Start an interactive demo with no input file:

```sh
typesafe-ai parts-of-speech-serve
# Open http://127.0.0.1:8231
```

Type or paste text into the freeform field and press **Parse** (or Ctrl/⌘+Enter).
The results update in place, with a per-parse cost/token summary and Luna comparison.
Parsing happens only on submission. While parsing, the form shows a busy state;
errors preserve the text and previous results. Color and uncertainty settings
carry over to newly parsed results.

The server uses `OPENROUTER_API_KEY` or `TYPESAFE_API_KEY` from its own environment;
credentials never enter the browser. `--model` / `TYPESAFE_MODEL` selects the model.
Startup and entirely local classifications need no credentials. Requests that
need Jev report missing credentials or API failures in the page. Parsing uses the
same compact batching and local-word shortcuts as the CLI command.

Save the analysis once, then explore it without further API calls:

```sh
typesafe-ai parts-of-speech passage.txt --cost > analysis.json
typesafe-ai parts-of-speech-serve analysis.json
# Open http://127.0.0.1:8231
```

Or pipe saved results directly (the viewer reads until EOF before starting):

```sh
typesafe-ai parts-of-speech passage.txt | typesafe-ai parts-of-speech-serve
```

Saved or piped results seed the viewer; the input field can then parse a new passage.
With no file, terminal stdin or empty redirected stdin opens the empty demo.
Explicit `-` still requires JSON on stdin.

The responsive viewer arranges each sentence into colored word boxes with visible
labels. Switch between **word type** and **grammatical role**, or highlight tokens
where either model judgment has confidence below 60%. Hover, keyboard-focus, or
tap a word to see both full labels, separate confidence values, and its sentence
and token indices. Locally classified axes say “Local rule” rather than displaying
a model confidence. Escape dismisses the tooltip. Punctuation is retained in the
same reading order. “Fully local” counts tokens, including punctuation, for which
neither axis has a model confidence.

Use `--bind 127.0.0.1:9000` to change the address. Export a self-contained HTML page
for opening directly or serving with any static web server:

```sh
typesafe-ai parts-of-speech-serve analysis.json --html > analysis.html
```

The HTML export is a read-only viewer of saved analysis; `--html` requires JSON
input and hides the parsing form. It includes its CSS and JavaScript, requires no
external assets or API keys, and escapes all input text and labels. Malformed
saved JSON, empty sentences, and out-of-range confidence values are rejected
before the server starts.

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

Each file's full source is sent once in one TypeSafe request,
with **two independent Score questions per comment**:

- **Accuracy:** How accurately does the comment describe the following code? Inline
  comments also consider their associated code on the same line.
- **Usefulness:** How much does it add useful understanding, explain non-obvious code, or
  capture historical or human reasons?

The two question definitions and their rubric levels are supplied once in shared
state. Per-comment instructions are compact calls such as `a(2,2)` (accuracy)
and `b(2,2)` (usefulness), with inclusive 1-based line ranges. Each Score's
required criteria array references the shared rubric levels. This notation is
explained to Jev in state; it is not an API-level template or reference feature.

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

## Load-bearing source heat map

```sh
typesafe-ai load-bearing example.ts > scores.jsonl
typesafe-ai load-bearing ./src --model jev-latest > scores.jsonl
typesafe-ai load-bearing-serve scores.jsonl
# Or pipe directly (the viewer starts after scoring reaches EOF):
typesafe-ai load-bearing ./src | typesafe-ai load-bearing-serve
```

Open **http://127.0.0.1:8230**. The viewer renders complete files with offline,
server-side syntax highlighting (including Rust, JS/TS/TSX, Python, HTML, JSON,
and Markdown; unknown formats fall back to plain text). Important lines stay sharp
and bright with mint-green backgrounds; incidental lines are dim and increasingly
blurred only below a score of 0.30, reaching 1.2px blur at zero. Lines scoring
0.30 or higher remain sharp. Hover to brighten and sharpen any line,
toggle the heat map to read normally, and use the file navigation or line links.
The gutter displays line numbers only. Stop the server with Ctrl-C.

Scoring rates each line's direct contribution to distinctive runtime behavior.
Type-only interfaces, aliases, annotations, and declaration-only signatures belong
at zero; naming an important operation does not inherit its implementation's
importance. Mixed code/type lines are judged on their executable content.
Lines are judged through the purpose of their enclosing operation: diagnostic
logging arguments remain instrumentation even when they reference important
business data. Durable audit records and behavior-driving events are distinguished
by their role, not their method names. For JS/TS files, Tree-sitter supplies the
enclosing statement explicitly alongside the full source. All scores remain model
judgments; logging scores are not clamped or overridden.

Scoring emits one JSON object per file, flushed in completion order:

```json
{"file":"src/example.ts","source":"const value = compute();\n}\n","score":{"1":0.85}}
```

`score` keys are **1-based physical line numbers** and values are **0–1 importance
ratings**, not probabilities. Omitted lines count as zero. Lines with fewer than
four ASCII letters (`a-zA-Z`) outside comments are skipped. Syntax-aware comment
detection omits comment-only lines, including multiline comments, while keeping
code with trailing comments and comment-like text inside strings. Unknown file
types are treated as plain text. Comments remain in the full-file context and
viewer. Files with no eligible lines need no
API call. Empty files yield an empty score map. Source snapshots preserve the exact
text that was evaluated, even if the files later change.

Load-bearing likewise defines its question and five rubric levels once in shared
state, with per-line instructions such as `a(3,3)` and short rubric references.
Each eligible line gets its own Score question in a single batched request per
file, sharing the full file as context. Questions run independently in parallel;
all file requests launch concurrently. The five-level rubric runs from no runtime
contribution through routine plumbing and supporting operations to key runtime
decisions and defining operations or invariants.
Scores are divided by four to normalize them. Mere syntax breakage on deletion
does not make a line semantically important. Full files are never truncated and
must fit the model's request limits.

Directory behavior matches `phi`: immediate regular files (including hidden
files), no recursive traversal or symlink following. UTF-8/binary and API failures
are reported on stderr, other files continue, and any failure causes a nonzero
exit. Cost reporting stays on stderr so stdout remains valid JSONL.

The server also accepts minimal records like
`{"file":"src/example.ts","score":{"1":0.85}}`, reading their files relative to
`--root .`. With snapshots it needs no access to the original files or API key.
Use `--bind 127.0.0.1:9000` to change the listen address. JSONL is loaded once at
startup; restart to load new results.

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
empty input clears the marker and score text and is not scored. With `--cost`,
the exit summary counts completed requests, including superseded ones.
Nerd Font icons render best with a Nerd Font terminal font; text labels are also
shown.

Use arrow keys or **Ctrl-B/F** to move, **Home/End** or **Ctrl-A/E** for start/end,
**Alt-B/F** to move by word, **Ctrl-U/K/W** to cut, and **Ctrl-Y** to yank.
**Backspace/Ctrl-H** deletes left; **Delete/Ctrl-D** deletes right. Pasted newlines
are converted to spaces. **F1** shows editing help, **Esc** closes help, and
**Ctrl-C** quits. Ordinary `q`, `j`, and `?` remain text input.

Exiting restores the terminal. When `--cost` is set, the summary is printed to
stderr; requests still in flight are reported as unknown rather than blocking
terminal exit. `NO_COLOR` disables the color scale.

## Business and occupation classification

```sh
typesafe-ai business
typesafe-ai job
```

Both are interactive Rust terminal views. Type or paste a free-form description,
then press **Enter** to classify it. Typing alone makes no API calls. `business` classifies the principal
revenue-producing activity using the **2025 IRS Schedule C code list**;
`job` classifies a person's duties using **O*NET® 31.0 occupation codes**.

Each view shows the selected official code/title, its description where provided,
and the top five candidates with probability bars. The previous result remains
visible during edits; clearing the text clears results. They share `be-nice`'s
editing controls, F1 help, Ctrl-C exit, color settings, and cost-on-exit reporting.

Classification first compares broad categories, then compares codes from the
two leading categories. Candidate pools larger than 253 codes are recursively
split in half and narrowed by additional Choices. Every Choice has at most 255
options, including **insufficient information** and **no matching option**.
An obsolete classification stops before its next API call, and late responses
cannot replace current results.

Displayed probabilities come from the final Choice **among the shortlisted
candidates**, not a distribution over the entire catalog. Category pruning can
miss a relevant code; the explicit no-match option allows the model to say so.
Codes and labels always come from bundled data, never generated text.

Sources, versions, attribution, and the Rust regeneration command are documented
in [`data/README.md`](data/README.md).

## Configuration

Pass the global `--cost` flag before or after any subcommand to print one cost
summary to **stderr** when finished, for example:

```sh
typesafe-ai --cost phi patient-notes.txt
typesafe-ai phi patient-notes.txt --cost
```

```text
Cost: 0.0042¢
Tokens: 1000
Luna Cost: 0.0236¢ (5.6× Jev)
```

Without `--cost`, no cost or token summary is printed.

Cost is calculated from the API's `usage.input_tokens` at **$0.042 per million
input tokens** (4.2 US pennies per million). Output tokens are free. Input usage
is summed across all requests, including concurrent files and any retry responses
that report usage, then rounded once to four decimal places. A batched comment
request is counted once, not once per question. No queries means `Cost: 0.0000¢`
and `Tokens: 0`.

`Luna Cost:` estimates the same workload at OpenAI GPT-5.6 Luna's standard
[pricing](https://developers.openai.com/api/docs/models/gpt-5.6-luna) (checked
September 19, 2026): **$0.20/M input tokens and $1.20/M output tokens**.
It reuses Jev's reported input count and uses `tiktoken-rs`'s OpenAI `o200k_base`
tokenizer on each compact `answers` JSON object, excluding response metadata.
The example above assumes 30 output tokens. Requests above 272,000 input tokens
use Luna's long-context rates ($0.40/M input, $1.80/M output). This is an
estimate without caching, batch discounts, or additional reasoning tokens;
it makes no OpenAI requests. Missing input usage or answers makes the Luna total
unavailable, with the known subtotal shown. No queries means `Luna Cost: 0.0000¢`.
The multiplier compares Luna to Jev using unrounded costs, displayed to one decimal
place; it is omitted when Jev's cost is zero or either total is unavailable.

The documented API provides token counts rather than a monetary cost field.
If a request's usage is missing (including a failed request without usage), the
summary reports the totals as unavailable and shows the known cost and token
count separately.
Normal results remain on stdout.

| Environment variable | Purpose | Default |
| --- | --- | --- |
| `OPENROUTER_API_KEY` | OpenRouter API key; takes precedence when set | — |
| `TYPESAFE_API_KEY` | TypeSafe API key used when no OpenRouter key is set | — |
| `TYPESAFE_MODEL` | Model; overridden by `--model` | `jev-latest` |
| `OPENROUTER_ENDPOINT` | Full OpenRouter Decisions endpoint URL | `https://openrouter.ai/api/alpha/decisions` |
| `TYPESAFE_ENDPOINT` | Full evaluation endpoint URL | `https://api.typesafe.ai/v1/systemone` |

For OpenRouter requests, `jev-latest` is sent as `typesafe/jev-1.13`; bare
versioned names such as `jev-1.13` are prefixed with `typesafe/`.

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
