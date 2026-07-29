use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

use crate::model::CycleMetadata;

pub fn load_cycle_metadata(db_path: &Path) -> Result<CycleMetadata> {
    let connection = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to open Fenix database: {}", db_path.display()))?;
    let mut statement = connection
        .prepare("SELECT key, val FROM config")
        .context("failed to query Fenix config")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .context("failed to read Fenix config")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to decode Fenix config")?;

    parse_cycle_metadata(
        rows.iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    )
}

pub fn parse_cycle_metadata<'a>(
    rows: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<CycleMetadata> {
    let mut cycle_name = None;
    let mut start_date = None;
    let mut end_date = None;

    for (key, value) in rows {
        match key {
            "CycleName" => cycle_name = Some(value),
            "CycleStartDate" => start_date = Some(value),
            "CycleEndDate" => end_date = Some(value),
            _ => {}
        }
    }

    let cycle_name = cycle_name.context("Fenix config is missing CycleName")?;
    let (cycle, revision) = cycle_name
        .split_once('n')
        .map_or((cycle_name, "1"), |(cycle, revision)| (cycle, revision));
    if cycle.len() != 4 || !cycle.chars().all(|character| character.is_ascii_digit()) {
        bail!("invalid Fenix CycleName: {cycle_name}");
    }
    if revision.is_empty() || !revision.chars().all(|character| character.is_ascii_digit()) {
        bail!("invalid Fenix cycle revision: {cycle_name}");
    }

    Ok(CycleMetadata {
        cycle: cycle.to_string(),
        revision: revision.to_string(),
        start_date: start_date
            .context("Fenix config is missing CycleStartDate")?
            .to_string(),
        end_date: end_date
            .context("Fenix config is missing CycleEndDate")?
            .to_string(),
    })
}
