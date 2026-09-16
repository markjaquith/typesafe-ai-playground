# PHI probability coverage corpus

50 text files: two original examples preserved byte-for-byte and 48 additional
documents with exactly 20 paragraphs each. All identities, contact details,
clinical events, and institutions in the **new** documents are invented.
Treat those documents as if they were encountered in the stated setting when
evaluating detection; the corpus-level synthetic provenance is not an in-document
claim that every apparent chart is merely a training example.

## Intended coverage

There are five files targeting each probability decile, including the original
negative and positive examples at the endpoints. These are **authoring targets,
not measured model scores or legally adjudicated labels**. No classifier was run.
Actual coverage requires scoring with a fixed question/model and adjusting the
fixtures against the observed distribution.

The intended question is whether the document contains individually identifiable
human health information in a healthcare, care-coordination, or payment context.
Borderline cases explore missing provenance, uncertain linkage, partial identifiers,
rare combinations, research codes, personal disclosures, and nonclinical contexts.
A broader sensitive-health-data definition can score these differently from a
strict HIPAA PHI definition. Probability is uncertainty about presence, not the
percentage of paragraphs containing medical words: one clear disclosure suffices.

New filenames are neutral so their names do not disclose expected probabilities.
Pass only a sample's contents to a classifier, not this index. The original names
are retained as requested. Shared genre-specific background provides controlled
distractors; eight scenario-specific paragraphs in each new file carry its facts,
with varied positions throughout the document. These fixtures are a coverage
exercise, not an independent statistical calibration or train/test benchmark.

Intervals below are lower-inclusive and upper-exclusive, except the final interval
includes 100%. The two original files retain their original two-paragraph length.

| File | Target probability | Scenario | Document genre |
| --- | --- | --- | --- |
| no-phi.txt | 0–10% | Original gardening example | General |
| definitely-phi.txt | 90–100% | Original named clinical record | Clinical |
| sample-001.txt | 0–10% | seed library | general |
| sample-002.txt | 0–10% | railway layout | general |
| sample-003.txt | 0–10% | bread workshop | general |
| sample-004.txt | 0–10% | astronomy club | general |
| sample-005.txt | 10–20% | blood pressure handout | education |
| sample-006.txt | 10–20% | hospital loading dock | operations |
| sample-007.txt | 10–20% | regional health summary | research |
| sample-008.txt | 10–20% | blank referral form | education |
| sample-009.txt | 10–20% | clinic furniture catalog | general |
| sample-010.txt | 20–30% | simulated chart | education |
| sample-011.txt | 20–30% | public donation story | correspondence |
| sample-012.txt | 20–30% | anonymous nutrition survey | research |
| sample-013.txt | 20–30% | animal clinic news | general |
| sample-014.txt | 20–30% | deidentified teaching case | education |
| sample-015.txt | 30–40% | running club message | correspondence |
| sample-016.txt | 30–40% | workplace absence | operations |
| sample-017.txt | 30–40% | unlinked case narrative | research |
| sample-018.txt | 30–40% | roleplay or transcript | education |
| sample-019.txt | 30–40% | appointment token | operations |
| sample-020.txt | 40–50% | small group wellness | research |
| sample-021.txt | 40–50% | ambiguous initials | correspondence |
| sample-022.txt | 40–50% | unknown code export | research |
| sample-023.txt | 40–50% | occupational first aid | operations |
| sample-024.txt | 40–50% | support chat handle | correspondence |
| sample-025.txt | 50–60% | redacted clinic roster | operations |
| sample-026.txt | 50–60% | rare case coarse location | research |
| sample-027.txt | 50–60% | possible patient email | correspondence |
| sample-028.txt | 50–60% | training copy unclear origin | education |
| sample-029.txt | 50–60% | study key uncertain | research |
| sample-030.txt | 60–70% | initials room service | operations |
| sample-031.txt | 60–70% | coded record retained key | research |
| sample-032.txt | 60–70% | clinic voicemail | correspondence |
| sample-033.txt | 60–70% | invoice treatment code | operations |
| sample-034.txt | 60–70% | portal reset | correspondence |
| sample-035.txt | 70–80% | partial name discharge | operations |
| sample-036.txt | 70–80% | lab accession | clinical |
| sample-037.txt | 70–80% | caregiver message | correspondence |
| sample-038.txt | 70–80% | device telemetry | research |
| sample-039.txt | 70–80% | pharmacy pickup | operations |
| sample-040.txt | 80–90% | named diabetes review | clinical |
| sample-041.txt | 80–90% | insurance authorization | operations |
| sample-042.txt | 80–90% | mental health followup | correspondence |
| sample-043.txt | 80–90% | prenatal summary | clinical |
| sample-044.txt | 80–90% | emergency injury | clinical |
| sample-045.txt | 90–100% | infectious disease record | clinical |
| sample-046.txt | 90–100% | oncology treatment | clinical |
| sample-047.txt | 90–100% | psychiatric discharge | clinical |
| sample-048.txt | 90–100% | renal care plan | clinical |
