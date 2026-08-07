use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use fenix_to_tfdi::cli::parse_conversion_args;
use rusqlite::Connection;

const REQUIRED_TABLES: &[&str] = &[
    "AirportCommunication",
    "AirportLookup",
    "Airports",
    "AirwayLegs",
    "Airways",
    "config",
    "Gls",
    "GridMora",
    "Holdings",
    "ILSes",
    "Markers",
    "MarkerTypes",
    "NavaidLookup",
    "Navaids",
    "NavaidTypes",
    "Runways",
    "SurfaceTypes",
    "TerminalLegs",
    "TerminalLegsEx",
    "Terminals",
    "TrmLegTypes",
    "WaypointLookup",
    "Waypoints",
];

#[derive(Clone, Debug)]
pub(crate) struct AppConfig {
    pub(crate) output_targets: Vec<OutputLocation>,
    pub(crate) reference_dir: Option<PathBuf>,
    pub(crate) db_path: Option<PathBuf>,
    pub(crate) start_terminal_id: Option<i64>,
    pub(crate) rte_seg_path: Option<PathBuf>,
    pub(crate) validate_only: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct OutputLocation {
    pub(crate) label: String,
    pub(crate) path: PathBuf,
}

pub(crate) fn parse_args() -> Result<AppConfig> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        std::process::exit(0);
    }
    if args.is_empty() {
        bail!("explicit conversion paths are required; run with --help for usage");
    }
    if let [option, candidate_dir] = args.as_slice()
        && option == "--validate"
    {
        return Ok(AppConfig {
            output_targets: Vec::new(),
            reference_dir: None,
            db_path: None,
            start_terminal_id: None,
            rte_seg_path: None,
            validate_only: Some(PathBuf::from(candidate_dir)),
        });
    }
    let request = parse_conversion_args(args)?;
    Ok(AppConfig {
        output_targets: vec![OutputLocation {
            label: "candidate output".to_string(),
            path: request.output_dir,
        }],
        reference_dir: Some(request.reference_dir),
        db_path: Some(request.db_path),
        start_terminal_id: None,
        rte_seg_path: Some(request.rte_seg_path),
        validate_only: None,
    })
}

fn print_help() {
    println!(
        "Fenix to TFDI navigation data converter\n\n\
Usage:\n  fenix_to_tfdi [OPTIONS]\n\n\
Options:\n  --db <PATH>         Fenix nd.db3 input\n  --rte-seg <PATH>    NAIP RTE_SEG.csv input\n  --reference <DIR>   Official TFDI Nav-Primary template\n  --output <DIR>      Isolated candidate output directory\n  --validate <DIR>    Validate an existing isolated candidate directory\n  -h, --help          Print help"
    );
}

pub(crate) fn prepare_output_directory(output_dir: &Path) -> Result<()> {
    fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "failed to create output directory: {}",
            output_dir.display()
        )
    })?;
    fs::create_dir_all(output_dir.join("ProcedureLegs")).with_context(|| {
        format!(
            "failed to create ProcedureLegs directory: {}",
            output_dir.display()
        )
    })?;
    Ok(())
}

pub(crate) fn prompt_db3_path() -> Result<PathBuf> {
    loop {
        let input = prompt("Enter Fenix nd.db3 path: ")?;
        let trimmed = input.trim().trim_matches(['\'', '"']);
        let path = PathBuf::from(trimmed);
        if !path.exists() || path.extension().and_then(|ext| ext.to_str()) != Some("db3") {
            println!("Invalid db3 path. Please enter a valid nd.db3 file.");
            continue;
        }

        let validation_conn = match Connection::open(&path) {
            Ok(conn) => conn,
            Err(error) => {
                println!("Failed to open database: {error}. Please try again.");
                continue;
            }
        };

        match validate_required_tables(&validation_conn) {
            Ok(()) => return Ok(path),
            Err(_) => println!("This file is not a valid Fenix nav database. Please try again."),
        }
    }
}

pub(crate) fn prompt_rte_seg_path() -> Result<PathBuf> {
    loop {
        let input = prompt("Enter RTE_SEG.csv path: ")?;
        let trimmed = input.trim().trim_matches(['\'', '"']);
        let path = PathBuf::from(trimmed);
        if !path.exists() || path.extension().and_then(|ext| ext.to_str()) != Some("csv") {
            println!("Invalid CSV path. Please enter a valid RTE_SEG.csv file.");
            continue;
        }
        return Ok(path);
    }
}

pub(crate) fn detect_start_terminal_id(output_dir: &Path) -> Result<i64> {
    let procedure_legs_dir = output_dir.join("ProcedureLegs");
    if !procedure_legs_dir.exists() {
        return Ok(1);
    }

    let mut max_terminal_id = 0i64;
    for entry in fs::read_dir(&procedure_legs_dir).with_context(|| {
        format!(
            "failed to read ProcedureLegs directory: {}",
            procedure_legs_dir.display()
        )
    })? {
        let entry = entry.with_context(|| {
            format!(
                "failed to read ProcedureLegs entry: {}",
                procedure_legs_dir.display()
            )
        })?;
        let Some(file_name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some(number_text) = file_name
            .strip_prefix("TermID_")
            .and_then(|value| value.strip_suffix(".json"))
        else {
            continue;
        };
        let Ok(value) = number_text.parse::<i64>() else {
            continue;
        };
        max_terminal_id = max_terminal_id.max(value);
    }

    Ok(max_terminal_id + 1)
}

pub(crate) fn validate_required_tables(conn: &Connection) -> Result<()> {
    let mut statement = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .context("failed to inspect sqlite schema")?;
    let table_rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .context("failed to iterate sqlite schema")?;
    let tables: HashSet<String> = table_rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to read sqlite schema rows")?
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    let missing: Vec<&str> = REQUIRED_TABLES
        .iter()
        .copied()
        .filter(|table| !tables.contains(&table.to_ascii_lowercase()))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        bail!("missing required tables: {}", missing.join(", "));
    }
}

fn prompt(message: &str) -> Result<String> {
    print!("{message}");
    io::stdout().flush().context("failed to flush stdout")?;
    let mut buffer = String::new();
    io::stdin()
        .read_line(&mut buffer)
        .context("failed to read user input")?;
    Ok(buffer)
}
