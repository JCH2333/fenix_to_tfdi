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
struct IdRow {
    #[serde(rename = "ID")]
    id: u64,
}

#[derive(Deserialize)]
struct RunwayAirportReference {
    #[serde(rename = "ID")]
    id: u64,
    #[serde(rename = "AirportID")]
    airport_id: u64,
}

#[derive(Deserialize)]
struct TerminalAirportReference {
    #[serde(rename = "ID")]
    id: u64,
    #[serde(rename = "AirportID")]
    airport_id: u64,
    #[serde(rename = "RwyID")]
    runway_id: Option<u64>,
}

#[derive(Deserialize)]
struct IlsRunwayReference {
    #[serde(rename = "ID")]
    id: u64,
    #[serde(rename = "RunwayID")]
    runway_id: u64,
}

#[derive(Deserialize)]
struct WaypointNavaidReference {
    #[serde(rename = "ID")]
    id: u64,
    #[serde(rename = "NavaidID")]
    navaid_id: Option<u64>,
}

#[derive(Deserialize)]
struct AirwayLegReference {
    #[serde(rename = "ID")]
    id: u64,
    #[serde(rename = "AirwayID")]
    airway_id: u64,
    #[serde(rename = "Waypoint1ID")]
    waypoint1_id: u64,
    #[serde(rename = "Waypoint2ID")]
    waypoint2_id: u64,
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

    let mut airport_ids = HashSet::new();
    let mut runway_ids = HashSet::new();
    let mut terminal_ids = HashSet::new();
    let mut navaid_ids = HashSet::new();
    let mut waypoint_ids = HashSet::new();
    let mut airway_ids = HashSet::new();
    for file_name in [
        "Airports.json",
        "Runways.json",
        "Terminals.json",
        "Navaids.json",
        "Waypoints.json",
        "Airways.json",
        "AirwayLegs.json",
        "ILSes.json",
    ] {
        let ids = load_unique_ids(&candidate_dir.join(file_name))?;
        match file_name {
            "Airports.json" => airport_ids = ids,
            "Runways.json" => runway_ids = ids,
            "Terminals.json" => terminal_ids = ids,
            "Navaids.json" => navaid_ids = ids,
            "Waypoints.json" => waypoint_ids = ids,
            "Airways.json" => airway_ids = ids,
            _ => {}
        }
    }

    validate_lookup_ids(
        &candidate_dir.join("NavaidLookup.json"),
        &navaid_ids,
        "Navaids.json",
    )?;
    validate_lookup_ids(
        &candidate_dir.join("WaypointLookup.json"),
        &waypoint_ids,
        "Waypoints.json",
    )?;

    let runways_path = candidate_dir.join("Runways.json");
    let runways: Vec<RunwayAirportReference> = serde_json::from_reader(
        fs::File::open(&runways_path)
            .with_context(|| format!("failed to open {}", runways_path.display()))?,
    )
    .with_context(|| format!("failed to parse references from {}", runways_path.display()))?;
    for runway in runways {
        if !airport_ids.contains(&runway.airport_id) {
            bail!(
                "{} runway ID {} references missing AirportID {}",
                runways_path.display(),
                runway.id,
                runway.airport_id
            );
        }
    }

    let terminals_path = candidate_dir.join("Terminals.json");
    let terminals: Vec<TerminalAirportReference> = serde_json::from_reader(
        fs::File::open(&terminals_path)
            .with_context(|| format!("failed to open {}", terminals_path.display()))?,
    )
    .with_context(|| {
        format!(
            "failed to parse references from {}",
            terminals_path.display()
        )
    })?;
    for terminal in terminals {
        if !airport_ids.contains(&terminal.airport_id) {
            bail!(
                "{} terminal ID {} references missing AirportID {}",
                terminals_path.display(),
                terminal.id,
                terminal.airport_id
            );
        }
        if let Some(runway_id) = terminal.runway_id
            && !runway_ids.contains(&runway_id)
        {
            bail!(
                "{} terminal ID {} references missing RwyID {}",
                terminals_path.display(),
                terminal.id,
                runway_id
            );
        }
    }

    let ils_path = candidate_dir.join("ILSes.json");
    let ils_rows: Vec<IlsRunwayReference> = serde_json::from_reader(
        fs::File::open(&ils_path)
            .with_context(|| format!("failed to open {}", ils_path.display()))?,
    )
    .with_context(|| format!("failed to parse references from {}", ils_path.display()))?;
    for ils in ils_rows {
        if !runway_ids.contains(&ils.runway_id) {
            bail!(
                "{} ILS ID {} references missing RunwayID {}",
                ils_path.display(),
                ils.id,
                ils.runway_id
            );
        }
    }

    let waypoints_path = candidate_dir.join("Waypoints.json");
    let waypoints: Vec<WaypointNavaidReference> = serde_json::from_reader(
        fs::File::open(&waypoints_path)
            .with_context(|| format!("failed to open {}", waypoints_path.display()))?,
    )
    .with_context(|| {
        format!(
            "failed to parse references from {}",
            waypoints_path.display()
        )
    })?;
    for waypoint in waypoints {
        if let Some(navaid_id) = waypoint.navaid_id
            && !navaid_ids.contains(&navaid_id)
        {
            bail!(
                "{} waypoint ID {} references missing NavaidID {}",
                waypoints_path.display(),
                waypoint.id,
                navaid_id
            );
        }
    }

    let airway_legs_path = candidate_dir.join("AirwayLegs.json");
    let airway_legs: Vec<AirwayLegReference> = serde_json::from_reader(
        fs::File::open(&airway_legs_path)
            .with_context(|| format!("failed to open {}", airway_legs_path.display()))?,
    )
    .with_context(|| {
        format!(
            "failed to parse references from {}",
            airway_legs_path.display()
        )
    })?;
    for leg in airway_legs {
        if !airway_ids.contains(&leg.airway_id) {
            bail!(
                "{} airway leg ID {} references missing AirwayID {}",
                airway_legs_path.display(),
                leg.id,
                leg.airway_id
            );
        }
        if !waypoint_ids.contains(&leg.waypoint1_id) {
            bail!(
                "{} airway leg ID {} references missing Waypoint1ID {}",
                airway_legs_path.display(),
                leg.id,
                leg.waypoint1_id
            );
        }
        if !waypoint_ids.contains(&leg.waypoint2_id) {
            bail!(
                "{} airway leg ID {} references missing Waypoint2ID {}",
                airway_legs_path.display(),
                leg.id,
                leg.waypoint2_id
            );
        }
    }

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

fn load_unique_ids(path: &Path) -> Result<HashSet<u64>> {
    let rows: Vec<IdRow> = serde_json::from_reader(
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse IDs from {}", path.display()))?;
    let mut ids = HashSet::with_capacity(rows.len());
    for row in rows {
        if !ids.insert(row.id) {
            bail!("{} contains duplicate ID {}", path.display(), row.id);
        }
    }
    Ok(ids)
}

fn validate_lookup_ids(path: &Path, primary_ids: &HashSet<u64>, primary_file: &str) -> Result<()> {
    let rows: Vec<IdRow> = serde_json::from_reader(
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse lookup IDs from {}", path.display()))?;
    for row in rows {
        if !primary_ids.contains(&row.id) {
            bail!(
                "{} lookup ID {} has no matching row in {}",
                path.display(),
                row.id,
                primary_file
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
