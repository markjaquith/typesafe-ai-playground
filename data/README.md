# Bundled classification catalogs

The CLI embeds these files: classification never downloads a taxonomy at runtime.

## Business

`business.json` contains all **304 six-digit codes** in the **2025 IRS Schedule C
Principal Business or Professional Activity Codes** chart, retrieved 2026-09-16.

Source: https://www.irs.gov/instructions/i1040sc

Titles and category headings are extracted from the official HTML, with entities
decoded and whitespace normalized. The `xx` nonstore-retail explanatory note is
not a code and is excluded. These are IRS Schedule C codes, not the full NAICS
catalog: for example, the IRS chart uses **541510**, not NAICS **541511**.

## Job

`job.json` includes all **1,016 occupations** from the **O*NET® 31.0 Database**,
by the U.S. Department of Labor, Employment and Training Administration (USDOL/ETA).
Used under the [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) license.
O*NET® is a trademark of USDOL/ETA.

Source: https://www.onetcenter.org/dl_files/database/db_31_0_json/occupation_data.json

The official codes, titles, and descriptions are retained; fields are renamed,
rows sorted, and a major-group label is added using the code's first two digits.
USDOL/ETA has not approved, endorsed, or tested these modifications.

The displayed codes are O*NET-SOC occupation codes (for example, `15-1252.00`),
not IRS codes or industry codes.

## Regeneration

```sh
cargo run --example import_taxonomies
```

The importer is Rust. It checks the expected IRS tax year, occupation row count,
and code uniqueness before writing the catalog snapshots. Review changes before
updating the pinned versions.
