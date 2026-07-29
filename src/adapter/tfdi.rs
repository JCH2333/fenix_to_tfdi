use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::Path;

use anyhow::Context;

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
