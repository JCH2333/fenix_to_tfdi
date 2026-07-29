use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, bail};

use crate::model::CycleMetadata;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TfdiCycleFiles {
    pub config_json: String,
    pub cycle_json: String,
}

#[derive(Serialize)]
struct ConfigEntry<'a> {
    key: &'a str,
    val: &'a str,
}

#[derive(Serialize)]
struct CycleFile<'a> {
    cycle: &'a str,
    revision: &'a str,
    name: &'static str,
}

#[derive(Deserialize)]
struct TerminalId {
    #[serde(rename = "ID")]
    id: u64,
}

pub fn validate_candidate(candidate_dir: &Path) -> Result<()> {
    for file_name in [
        "Config.json",
        "cycle.json",
        "Airports.json",
        "Runways.json",
        "Terminals.json",
        "Navaids.json",
        "NavaidLookup.json",
        "Waypoints.json",
        "WaypointLookup.json",
        "Airways.json",
        "AirwayLegs.json",
        "ILSes.json",
    ] {
        let path = candidate_dir.join(file_name);
        if !path.is_file() {
            bail!("required TFDI file is missing: {}", path.display());
        }
        serde_json::from_reader::<_, serde_json::Value>(
            fs::File::open(&path).with_context(|| {
                format!("failed to open required TFDI file: {}", path.display())
            })?,
        )
        .with_context(|| format!("failed to parse required TFDI file: {}", path.display()))?;
    }

    let terminals_path = candidate_dir.join("Terminals.json");
    let terminals: Vec<TerminalId> = serde_json::from_reader(
        fs::File::open(&terminals_path)
            .with_context(|| format!("failed to open {}", terminals_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", terminals_path.display()))?;
    let terminal_ids: HashSet<u64> = terminals.into_iter().map(|terminal| terminal.id).collect();

    let procedure_dir = candidate_dir.join("ProcedureLegs");
    for entry in fs::read_dir(&procedure_dir)
        .with_context(|| format!("failed to read {}", procedure_dir.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read directory entry under {}",
                procedure_dir.display()
            )
        })?;
        if !entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", entry.path().display()))?
            .is_file()
        {
            continue;
        }

        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let Some(id) = file_name
            .strip_prefix("TermID_")
            .and_then(|name| name.strip_suffix(".json"))
            .and_then(|id| id.parse::<u64>().ok())
        else {
            continue;
        };
        if !terminal_ids.contains(&id) {
            bail!(
                "procedure file {} has no matching terminal",
                entry.path().display()
            );
        }
    }

    Ok(())
}

pub fn render_cycle_files(cycle: &CycleMetadata) -> Result<TfdiCycleFiles> {
    let config = [
        ConfigEntry {
            key: "CycleEndDate",
            val: &cycle.end_date,
        },
        ConfigEntry {
            key: "CycleName",
            val: &cycle.cycle,
        },
        ConfigEntry {
            key: "CycleStartDate",
            val: &cycle.start_date,
        },
    ];
    let cycle_file = CycleFile {
        cycle: &cycle.cycle,
        revision: &cycle.revision,
        name: "TFDi Design MD-11",
    };

    Ok(TfdiCycleFiles {
        config_json: serde_json::to_string(&config)?,
        cycle_json: serde_json::to_string(&cycle_file)?,
    })
}

pub fn write_cycle_files(output_dir: &Path, cycle: &CycleMetadata) -> Result<()> {
    let files = render_cycle_files(cycle)?;
    replace_candidate_file(&output_dir.join("Config.json"), &files.config_json)?;
    replace_candidate_file(&output_dir.join("cycle.json"), &files.cycle_json)?;
    Ok(())
}

fn replace_candidate_file(path: &Path, contents: &str) -> Result<()> {
    let temporary = path.with_extension("fenix-to-tfdi.tmp");
    let swap = path.with_extension("fenix-to-tfdi.swap");
    fs::write(&temporary, contents)
        .with_context(|| format!("failed to write temporary file: {}", temporary.display()))?;

    if swap.exists() {
        fs::remove_file(&swap)
            .with_context(|| format!("failed to remove stale swap file: {}", swap.display()))?;
    }
    let had_existing = path.exists();
    if had_existing {
        fs::rename(path, &swap)
            .with_context(|| format!("failed to stage existing file: {}", path.display()))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        if had_existing {
            let _ = fs::rename(&swap, path);
        }
        return Err(error).with_context(|| format!("failed to replace {}", path.display()));
    }
    if had_existing {
        fs::remove_file(&swap)
            .with_context(|| format!("failed to remove swap file: {}", swap.display()))?;
    }
    Ok(())
}
