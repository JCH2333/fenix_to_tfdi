use anyhow::Result;
use serde::Serialize;

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

