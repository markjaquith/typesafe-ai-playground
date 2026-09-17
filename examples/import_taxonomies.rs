//! Regenerate bundled official data: cargo run --example import_taxonomies
use anyhow::{Context, Result, ensure};
use scraper::{Html, Selector};
use serde_json::{Value, json};
use std::{collections::HashSet, fs, time::Duration};

fn main() -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    let irs_url = "https://www.irs.gov/instructions/i1040sc";
    let html = client.get(irs_url).send()?.error_for_status()?.text()?;
    ensure!(
        html.contains("Instructions for Schedule C (Form 1040) (2025)"),
        "IRS tax year changed; review source before updating"
    );
    let document = Html::parse_document(&html);
    let selector = Selector::parse("h3.role-category, span.code, span.activity").unwrap();
    let mut group = String::new();
    let mut code = String::new();
    let mut business = Vec::new();
    for element in document.select(&selector) {
        let text = element
            .text()
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if element.value().name() == "h3" {
            group = text;
        } else if element.value().classes().any(|class| class == "code") {
            code = text.trim_end_matches(" -").to_owned();
        } else {
            if code == "xx" {
                continue;
            } // IRS explanatory note, not a selectable code.
            ensure!(
                code.len() == 6 && !group.is_empty(),
                "unexpected IRS code row: {code:?}, {group:?}, {text:?}"
            );
            business.push(json!({"code": code, "title": text, "description": "", "group": group}));
        }
    }
    ensure!(business.len() > 300, "incomplete IRS extraction");
    let onet_url = "https://www.onetcenter.org/dl_files/database/db_31_0_json/occupation_data.json";
    let onet: Value = client.get(onet_url).send()?.error_for_status()?.json()?;
    let groups = [
        ("11", "Management"),
        ("13", "Business and Financial Operations"),
        ("15", "Computer and Mathematical"),
        ("17", "Architecture and Engineering"),
        ("19", "Life, Physical, and Social Science"),
        ("21", "Community and Social Service"),
        ("23", "Legal"),
        ("25", "Educational Instruction and Library"),
        ("27", "Arts, Design, Entertainment, Sports, and Media"),
        ("29", "Healthcare Practitioners and Technical"),
        ("31", "Healthcare Support"),
        ("33", "Protective Service"),
        ("35", "Food Preparation and Serving Related"),
        ("37", "Building and Grounds Cleaning and Maintenance"),
        ("39", "Personal Care and Service"),
        ("41", "Sales and Related"),
        ("43", "Office and Administrative Support"),
        ("45", "Farming, Fishing, and Forestry"),
        ("47", "Construction and Extraction"),
        ("49", "Installation, Maintenance, and Repair"),
        ("51", "Production"),
        ("53", "Transportation and Material Moving"),
        ("55", "Military Specific"),
    ];
    let mut jobs = Vec::new();
    for row in onet["row"].as_array().context("missing O*NET rows")? {
        let code = row["onetsoc_code"]
            .as_str()
            .context("missing occupation code")?;
        let group = groups
            .iter()
            .find(|(prefix, _)| code.starts_with(prefix))
            .context("unknown SOC major group")?
            .1;
        jobs.push(json!({"code": code, "title": row["title"], "description": row["description"], "group": group}));
    }
    ensure!(jobs.len() == 1016, "unexpected O*NET 31.0 row count");
    fs::create_dir_all("data")?;
    for (name, version, source, mut rows) in [
        ("business", "IRS Schedule C 2025", irs_url, business),
        ("job", "O*NET 31.0", onet_url, jobs),
    ] {
        rows.sort_by(|a, b| a["code"].as_str().cmp(&b["code"].as_str()));
        let codes: HashSet<_> = rows
            .iter()
            .map(|row| row["code"].as_str().unwrap())
            .collect();
        ensure!(codes.len() == rows.len(), "duplicate codes in {name}");
        fs::write(
            format!("data/{name}.json"),
            serde_json::to_string_pretty(
                &json!({"version": version, "source": source, "entries": rows}),
            )? + "\n",
        )?;
        println!("{name}: {} codes", rows.len());
    }
    Ok(())
}
