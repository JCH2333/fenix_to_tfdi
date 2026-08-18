use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use rusqlite::{Connection, params_from_iter};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use serde::Deserialize;
use serde::ser::{Serialize, SerializeStruct, Serializer};
use serde_json::{Number, Value};

use crate::db_json::{
    configure_read_connection, json_to_i64, normalize_numberish_json, sql_value_to_json,
};
use crate::stats::{PhaseDurations, TerminalLegExportStats, TerminalLegTimingBreakdown};
use crate::terminal_filters::is_excluded_terminal;
use crate::waypoints::{NavaidIdIndex, ReferenceIdIndex, TerminalExportPlan, WaypointIdIndex};

const PROCEDURE_LEG_JSON_BUFFER_CAPACITY: usize = 16 * 1024;

thread_local! {
    static TERMINAL_LEG_JSON_SCRATCH: RefCell<Vec<u8>> =
        RefCell::new(Vec::with_capacity(PROCEDURE_LEG_JSON_BUFFER_CAPACITY));
}

fn fast_hash_map<K, V>() -> FxHashMap<K, V> {
    FxHashMap::default()
}

fn fast_hash_map_with_capacity<K, V>(capacity: usize) -> FxHashMap<K, V> {
    FxHashMap::with_capacity_and_hasher(capacity, FxBuildHasher)
}

fn fast_hash_set<T>() -> FxHashSet<T> {
    FxHashSet::default()
}

#[derive(Debug, Deserialize)]
struct ExistingTerminalFilterRow {
    #[serde(rename = "ID")]
    id: Option<i64>,
    #[serde(rename = "ICAO")]
    icao: Option<String>,
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "FullName")]
    full_name: Option<String>,
}

#[derive(Clone, Debug)]
struct TerminalLegRecord {
    id: i64,
    terminal_id: i64,
    leg_type: Value,
    transition: Value,
    track_code: Value,
    wpt_id: Value,
    wpt_id_num: Option<i64>,
    wpt_lat: Value,
    wpt_lon: Value,
    turn_dir: Value,
    nav_id: Value,
    nav_id_num: Option<i64>,
    nav_lat: Value,
    nav_lon: Value,
    nav_bear: Value,
    nav_dist: Value,
    course: Value,
    distance: Value,
    alt: Value,
    vnav: Value,
    center_id: Value,
    center_id_num: Option<i64>,
    center_lat: Value,
    center_lon: Value,
    is_fly_over: Value,
    is_fly_over_num: Option<i64>,
    speed_limit: Value,
    speed_limit_num: Option<i64>,
    wpt_desc_code: Value,
    is_faf: i64,
    is_map: i64,
}

impl TerminalLegRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let id = row.get::<_, i64>(0)?;
        let terminal_id = row.get::<_, i64>(1)?;
        let wpt_id = normalize_numberish_json(&sql_value_to_json(row.get(5)?));
        let nav_id = normalize_numberish_json(&sql_value_to_json(row.get(9)?));
        let alt = sql_value_to_json(row.get(16)?);
        let vnav = normalize_numberish_json(&sql_value_to_json(row.get(17)?));
        let center_id = normalize_numberish_json(&sql_value_to_json(row.get(18)?));
        let is_fly_over = normalize_numberish_json(&sql_value_to_json(row.get(21)?));
        let speed_limit = normalize_numberish_json(&sql_value_to_json(row.get(22)?));
        let wpt_desc_code = sql_value_to_json(row.get(23)?);
        let is_map = matches!(&alt, Value::String(text) if text == "MAP");
        Ok(Self {
            id,
            terminal_id,
            // TFDI stores procedure leg types as string enums, for example "5".
            leg_type: sql_value_to_json(row.get(2)?),
            transition: sql_value_to_json(row.get(3)?),
            track_code: sql_value_to_json(row.get(4)?),
            wpt_id_num: json_to_i64(Some(&wpt_id)),
            wpt_id,
            wpt_lat: normalize_numberish_json(&sql_value_to_json(row.get(6)?)),
            wpt_lon: normalize_numberish_json(&sql_value_to_json(row.get(7)?)),
            turn_dir: sql_value_to_json(row.get(8)?),
            nav_id_num: json_to_i64(Some(&nav_id)),
            nav_id,
            nav_lat: normalize_numberish_json(&sql_value_to_json(row.get(10)?)),
            nav_lon: normalize_numberish_json(&sql_value_to_json(row.get(11)?)),
            nav_bear: normalize_numberish_json(&sql_value_to_json(row.get(12)?)),
            nav_dist: normalize_numberish_json(&sql_value_to_json(row.get(13)?)),
            course: normalize_numberish_json(&sql_value_to_json(row.get(14)?)),
            distance: normalize_numberish_json(&sql_value_to_json(row.get(15)?)),
            alt,
            vnav,
            center_id_num: json_to_i64(Some(&center_id)),
            center_id,
            center_lat: normalize_numberish_json(&sql_value_to_json(row.get(19)?)),
            center_lon: normalize_numberish_json(&sql_value_to_json(row.get(20)?)),
            is_fly_over_num: json_to_i64(Some(&is_fly_over)),
            is_fly_over,
            speed_limit_num: json_to_i64(Some(&speed_limit)),
            speed_limit,
            wpt_desc_code,
            is_faf: 0,
            is_map: if is_map { -1 } else { 0 },
        })
    }

    fn fill_coordinates(
        &mut self,
        runway_ids_by_terminal: &FxHashMap<i64, i64>,
        runway_coords: &FxHashMap<i64, (f64, f64)>,
        waypoint_coords: &FxHashMap<i64, (f64, f64)>,
        navaid_coords: &FxHashMap<i64, (f64, f64)>,
    ) {
        let missing_wpt = value_is_null(&self.wpt_id)
            && value_is_null(&self.wpt_lat)
            && value_is_null(&self.wpt_lon);
        if missing_wpt && self.is_map == -1 {
            if let Some(runway_id) = runway_ids_by_terminal.get(&self.terminal_id)
                && let Some(coords) = runway_coords.get(runway_id)
            {
                self.wpt_lat = rounded_number_value(coords.0);
                self.wpt_lon = rounded_number_value(coords.1);
            }
            return;
        }

        if should_fill_value(&self.wpt_id, &self.wpt_lat, &self.wpt_lon) {
            if let Some(point_id) = self.wpt_id_num
                && let Some(coords) = waypoint_coords.get(&point_id)
            {
                self.wpt_lat = rounded_number_value(coords.0);
                self.wpt_lon = rounded_number_value(coords.1);
            }
            return;
        }

        if should_fill_value(&self.center_id, &self.center_lat, &self.center_lon) {
            if let Some(point_id) = self.center_id_num
                && let Some(coords) = waypoint_coords.get(&point_id)
            {
                self.center_lat = rounded_number_value(coords.0);
                self.center_lon = rounded_number_value(coords.1);
            }
            return;
        }

        if should_fill_value(&self.nav_id, &self.nav_lat, &self.nav_lon)
            && let Some(point_id) = self.nav_id_num
            && let Some(coords) = navaid_coords.get(&point_id)
        {
            self.nav_lat = rounded_number_value(coords.0);
            self.nav_lon = rounded_number_value(coords.1);
        }
    }

    fn remap_reference_ids(
        &mut self,
        waypoint_id_index: &WaypointIdIndex,
        navaid_id_index: &NavaidIdIndex,
    ) {
        remap_leg_id_value(&mut self.wpt_id, &mut self.wpt_id_num, waypoint_id_index);
        remap_leg_id_value(&mut self.nav_id, &mut self.nav_id_num, navaid_id_index);
        remap_leg_id_value(
            &mut self.center_id,
            &mut self.center_id_num,
            waypoint_id_index,
        );
    }
}

impl Serialize for TerminalLegRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("TerminalLegRecord", 25)?;
        state.serialize_field("ID", &self.id)?;
        state.serialize_field("TerminalID", &self.terminal_id)?;
        state.serialize_field("Type", &self.leg_type)?;
        state.serialize_field("Transition", &TextOrEmpty(&self.transition))?;
        state.serialize_field("TrackCode", &self.track_code)?;
        state.serialize_field("WptID", &self.wpt_id)?;
        state.serialize_field("WptLat", &self.wpt_lat)?;
        state.serialize_field("WptLon", &self.wpt_lon)?;
        state.serialize_field("TurnDir", &TextOrEmpty(&self.turn_dir))?;
        state.serialize_field("NavID", &self.nav_id)?;
        state.serialize_field("NavLat", &self.nav_lat)?;
        state.serialize_field("NavLon", &self.nav_lon)?;
        state.serialize_field("NavBear", &self.nav_bear)?;
        state.serialize_field("NavDist", &self.nav_dist)?;
        state.serialize_field("Course", &self.course)?;
        state.serialize_field("Distance", &self.distance)?;
        state.serialize_field("Alt", &TextOrEmpty(&self.alt))?;
        state.serialize_field("Vnav", &self.vnav)?;
        state.serialize_field("CenterID", &self.center_id)?;
        state.serialize_field("CenterLat", &self.center_lat)?;
        state.serialize_field("CenterLon", &self.center_lon)?;
        state.serialize_field(
            "IsFlyOver",
            &FlyOverOutput {
                value: &self.is_fly_over,
                numeric: self.is_fly_over_num,
            },
        )?;
        state.serialize_field(
            "SpeedLimit",
            &SpeedLimitOutput {
                value: &self.speed_limit,
                numeric: self.speed_limit_num,
            },
        )?;
        state.serialize_field("IsFAF", &self.is_faf)?;
        state.serialize_field("IsMAP", &self.is_map)?;
        state.end()
    }
}

struct TextOrEmpty<'a>(&'a Value);

impl Serialize for TextOrEmpty<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.0.is_null() {
            serializer.serialize_str("")
        } else {
            self.0.serialize(serializer)
        }
    }
}

struct FlyOverOutput<'a> {
    value: &'a Value,
    numeric: Option<i64>,
}

impl Serialize for FlyOverOutput<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.numeric == Some(1) {
            serializer.serialize_i64(-1)
        } else {
            self.value.serialize(serializer)
        }
    }
}

struct SpeedLimitOutput<'a> {
    value: &'a Value,
    numeric: Option<i64>,
}

impl Serialize for SpeedLimitOutput<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Some(value) = self.numeric {
            serializer.serialize_i64(value)
        } else {
            self.value.serialize(serializer)
        }
    }
}

pub(crate) fn export_terminal_legs(
    db_path: &Path,
    base_json_dir: Option<&Path>,
    start_terminal_id: i64,
    output_dir: &Path,
    waypoint_id_index: &WaypointIdIndex,
    navaid_id_index: &NavaidIdIndex,
    terminal_export_plan: &TerminalExportPlan,
) -> Result<TerminalLegExportStats> {
    let base_terminal_dir = base_json_dir.unwrap_or(output_dir);
    let source_terminal_id_map = terminal_export_plan.source_id_to_output_id();
    let source_terminal_ids = source_terminal_id_map
        .keys()
        .copied()
        .collect::<FxHashSet<_>>();
    let output_source_terminal_ids = source_terminal_id_map
        .values()
        .copied()
        .collect::<FxHashSet<_>>();
    let existing_terminal_ids = load_existing_terminal_ids(base_terminal_dir)?;
    let excluded_existing_terminal_ids = load_excluded_existing_terminal_ids(base_terminal_dir)?;
    let excluded_source_terminal_ids =
        with_connection(db_path, fetch_excluded_source_terminal_ids)?;
    let cleanup_required = procedure_dir_has_existing_files_from(output_dir, start_terminal_id)?
        || !excluded_existing_terminal_ids.is_empty()
        || !excluded_source_terminal_ids.is_empty();
    let files_may_already_exist = cleanup_required;
    let db_read_start = Instant::now();
    let (
        terminal_legs,
        runway_ids_by_terminal,
        terminal_ids_for_cleanup,
        runway_coords,
        waypoint_coords,
        navaid_coords,
        mut detail,
    ) = with_connection(db_path, |conn| {
        let t_terminal_legs = Instant::now();
        let mut terminal_legs = fetch_terminal_legs(conn, &source_terminal_ids)?;
        terminal_legs.retain(|leg| !excluded_source_terminal_ids.contains(&leg.terminal_id));
        remap_terminal_leg_terminal_ids(&mut terminal_legs, &source_terminal_id_map);
        let db_terminal_legs = t_terminal_legs.elapsed();

        let t_terminal_metadata = Instant::now();
        let runway_ids_by_source_terminal =
            fetch_runway_ids_by_terminal_ids(conn, &source_terminal_ids)?;
        let mut runway_ids_by_terminal = fast_hash_map();
        for (source_terminal_id, runway_id) in runway_ids_by_source_terminal {
            if excluded_source_terminal_ids.contains(&source_terminal_id) {
                continue;
            }
            if let Some(output_terminal_id) = source_terminal_id_map.get(&source_terminal_id) {
                runway_ids_by_terminal.insert(*output_terminal_id, runway_id);
            }
        }
        let terminal_ids_for_cleanup = if cleanup_required {
            Some(cleanup_allowed_terminal_ids(
                &existing_terminal_ids,
                &output_source_terminal_ids,
                &excluded_existing_terminal_ids,
            ))
        } else {
            None
        };
        let db_terminal_metadata = t_terminal_metadata.elapsed();

        let runway_ids: FxHashSet<i64> = runway_ids_by_terminal.values().copied().collect();
        recover_missing_rf_center_ids(conn, &mut terminal_legs)?;
        let waypoint_ids = collect_waypoint_reference_ids(&terminal_legs);
        let navaid_ids = collect_navaid_reference_ids(&terminal_legs);

        let t_runway_coords = Instant::now();
        let runway_coords = fetch_runway_coordinates_by_ids(conn, &runway_ids)?;
        let db_runway_coords = t_runway_coords.elapsed();

        let t_waypoint_coords = Instant::now();
        let waypoint_coords =
            fetch_waypoint_coordinates_by_db_ids(conn, &waypoint_ids, waypoint_id_index)?;
        let navaid_coords = fetch_navaid_coordinates_by_db_ids(conn, &navaid_ids, navaid_id_index)?;
        let db_waypoint_coords = t_waypoint_coords.elapsed();
        let mut terminal_legs = terminal_legs;
        remap_terminal_leg_reference_ids(&mut terminal_legs, waypoint_id_index, navaid_id_index);

        Ok::<_, anyhow::Error>((
            terminal_legs,
            runway_ids_by_terminal,
            terminal_ids_for_cleanup,
            runway_coords,
            waypoint_coords,
            navaid_coords,
            TerminalLegTimingBreakdown {
                db_terminal_legs,
                db_terminal_metadata,
                db_runway_coords,
                db_waypoint_coords,
                ..Default::default()
            },
        ))
    })?;
    let db_read_time = db_read_start.elapsed();
    let row_count = terminal_legs.len();

    let json_transform_time = std::time::Duration::default();
    detail.group_rows = std::time::Duration::default();

    let json_write_start = Instant::now();
    let file_count = write_terminal_leg_files(
        output_dir,
        files_may_already_exist,
        terminal_legs,
        &runway_ids_by_terminal,
        &runway_coords,
        &waypoint_coords,
        &navaid_coords,
    )?;

    if let Some(terminal_ids) = terminal_ids_for_cleanup {
        let cleanup_start = Instant::now();
        let _ = cleanup_extra_procedure_files(output_dir, &terminal_ids)?;
        detail.cleanup_files = cleanup_start.elapsed();
    }

    Ok(TerminalLegExportStats {
        row_count,
        file_count,
        phase: PhaseDurations {
            db_read: db_read_time,
            json_transform: json_transform_time,
            json_write: json_write_start.elapsed(),
        },
        detail,
    })
}

fn write_terminal_leg_files(
    output_dir: &Path,
    files_may_already_exist: bool,
    terminal_legs: Vec<TerminalLegRecord>,
    runway_ids_by_terminal: &FxHashMap<i64, i64>,
    runway_coords: &FxHashMap<i64, (f64, f64)>,
    waypoint_coords: &FxHashMap<i64, (f64, f64)>,
    navaid_coords: &FxHashMap<i64, (f64, f64)>,
) -> Result<usize> {
    let mut grouped_legs = group_terminal_legs_by_terminal(terminal_legs);
    let file_count = grouped_legs.len();
    let chunk_size = (grouped_legs.len() / (rayon::current_num_threads() * 4)).clamp(16, 128);

    grouped_legs
        .par_chunks_mut(chunk_size)
        .try_for_each(|chunk| {
            for (terminal_id, legs) in chunk {
                for leg in legs.iter_mut() {
                    leg.fill_coordinates(
                        runway_ids_by_terminal,
                        runway_coords,
                        waypoint_coords,
                        navaid_coords,
                    );
                }
                mark_final_approach_fix(legs);
                let output_path = output_dir
                    .join("ProcedureLegs")
                    .join(format!("TermID_{terminal_id}.json"));
                write_terminal_leg_json_file(&output_path, legs, files_may_already_exist)?;
            }
            Ok::<_, anyhow::Error>(())
        })?;

    Ok(file_count)
}

fn write_terminal_leg_json_file(
    output_path: &Path,
    legs: &[TerminalLegRecord],
    files_may_already_exist: bool,
) -> Result<()> {
    TERMINAL_LEG_JSON_SCRATCH.with(|scratch| {
        let mut encoded = scratch.borrow_mut();
        encoded.clear();
        serde_json::to_writer(&mut *encoded, legs).with_context(|| {
            format!(
                "failed to encode terminal leg json: {}",
                output_path.display()
            )
        })?;

        if files_may_already_exist
            && let Ok(metadata) = fs::metadata(output_path)
            && metadata.len() == encoded.len() as u64
            && let Ok(existing) = fs::read(output_path)
            && existing == *encoded
        {
            return Ok(());
        }

        fs::write(output_path, &*encoded).with_context(|| {
            format!(
                "failed to write terminal leg json: {}",
                output_path.display()
            )
        })
    })
}

fn remap_terminal_leg_terminal_ids(
    terminal_legs: &mut [TerminalLegRecord],
    source_terminal_id_map: &FxHashMap<i64, i64>,
) {
    for leg in terminal_legs {
        if let Some(output_terminal_id) = source_terminal_id_map.get(&leg.terminal_id) {
            leg.terminal_id = *output_terminal_id;
        }
    }
}

fn with_connection<T>(db_path: &Path, action: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("failed to open database: {}", db_path.display()))?;
    configure_read_connection(&conn);
    action(&conn)
}

fn fetch_terminal_legs(
    conn: &Connection,
    terminal_ids: &FxHashSet<i64>,
) -> Result<Vec<TerminalLegRecord>> {
    if terminal_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut statement = conn
        .prepare(
            "SELECT l.ID, l.TerminalID, l.Type, l.Transition, l.TrackCode, l.WptID, l.WptLat, l.WptLon, l.TurnDir, l.NavID, l.NavLat, l.NavLon, l.NavBear, l.NavDist, l.Course, l.Distance, l.Alt, l.Vnav, l.CenterID, l.CenterLat, l.CenterLon, ex.IsFlyOver, ex.SpeedLimit, l.WptDescCode FROM TerminalLegs l LEFT JOIN TerminalLegsEx ex ON ex.ID = l.ID ORDER BY l.TerminalID, l.ID",
        )
        .context("failed to query TerminalLegs")?;
    let rows = statement
        .query_map([], TerminalLegRecord::from_row)
        .context("failed to iterate TerminalLegs")?;
    let mut terminal_legs = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to read TerminalLegs")?;
    terminal_legs.retain(|leg| terminal_ids.contains(&leg.terminal_id));
    Ok(terminal_legs)
}

fn fetch_runway_ids_by_terminal_ids(
    conn: &Connection,
    terminal_ids: &FxHashSet<i64>,
) -> Result<FxHashMap<i64, i64>> {
    if terminal_ids.is_empty() {
        return Ok(fast_hash_map());
    }

    let mut statement = conn
        .prepare("SELECT ID, RwyID FROM Terminals WHERE RwyID IS NOT NULL")
        .context("failed to query terminal runway ids")?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
        .context("failed to iterate terminal runway ids")?;
    let mut runway_ids_by_terminal = fast_hash_map();
    for row in rows {
        let (terminal_id, runway_id) = row.context("failed to read terminal runway id row")?;
        if terminal_ids.contains(&terminal_id) {
            runway_ids_by_terminal.insert(terminal_id, runway_id);
        }
    }
    Ok(runway_ids_by_terminal)
}

fn load_excluded_existing_terminal_ids(output_dir: &Path) -> Result<FxHashSet<i64>> {
    let path = output_dir.join("Terminals.json");
    if !path.exists() {
        return Ok(fast_hash_set());
    }

    let bytes = fs::read(&path)
        .with_context(|| format!("failed to read existing terminals json: {}", path.display()))?;
    let rows: Vec<ExistingTerminalFilterRow> =
        serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "failed to parse existing terminals json: {}",
                path.display()
            )
        })?;

    Ok(rows
        .into_iter()
        .filter(|row| {
            is_excluded_terminal(
                row.icao.as_deref(),
                row.name.as_deref(),
                row.full_name.as_deref(),
            )
        })
        .filter_map(|row| row.id)
        .collect())
}

fn load_existing_terminal_ids(output_dir: &Path) -> Result<FxHashSet<i64>> {
    let path = output_dir.join("Terminals.json");
    if !path.exists() {
        return Ok(fast_hash_set());
    }

    let bytes = fs::read(&path)
        .with_context(|| format!("failed to read existing terminals json: {}", path.display()))?;
    let rows: Vec<ExistingTerminalFilterRow> =
        serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "failed to parse existing terminals json: {}",
                path.display()
            )
        })?;
    Ok(rows.into_iter().filter_map(|row| row.id).collect())
}

fn cleanup_allowed_terminal_ids(
    existing_terminal_ids: &FxHashSet<i64>,
    source_terminal_ids: &FxHashSet<i64>,
    excluded_existing_terminal_ids: &FxHashSet<i64>,
) -> FxHashSet<i64> {
    let mut allowed = existing_terminal_ids
        .iter()
        .filter(|id| !excluded_existing_terminal_ids.contains(id))
        .copied()
        .collect::<FxHashSet<_>>();
    allowed.extend(source_terminal_ids.iter().copied());
    allowed
}

fn group_terminal_legs_by_terminal(
    terminal_legs: Vec<TerminalLegRecord>,
) -> Vec<(i64, Vec<TerminalLegRecord>)> {
    let mut groups = Vec::new();
    let mut legs = terminal_legs.into_iter();
    let Some(first) = legs.next() else {
        return groups;
    };

    let mut current_terminal_id = first.terminal_id;
    let mut current_group = vec![first];
    for leg in legs {
        if leg.terminal_id == current_terminal_id {
            current_group.push(leg);
        } else {
            groups.push((current_terminal_id, current_group));
            current_terminal_id = leg.terminal_id;
            current_group = vec![leg];
        }
    }
    groups.push((current_terminal_id, current_group));
    groups
}

fn procedure_dir_has_existing_files_from(output_dir: &Path, min_terminal_id: i64) -> Result<bool> {
    let procedure_dir = output_dir.join("ProcedureLegs");
    if !procedure_dir.exists() {
        return Ok(false);
    }

    for entry in fs::read_dir(&procedure_dir).with_context(|| {
        format!(
            "failed to read ProcedureLegs directory: {}",
            procedure_dir.display()
        )
    })? {
        let entry = entry.with_context(|| {
            format!(
                "failed to read ProcedureLegs entry: {}",
                procedure_dir.display()
            )
        })?;
        let Ok(file_name) = entry.file_name().into_string() else {
            continue;
        };
        let Some(number_text) = file_name
            .strip_prefix("TermID_")
            .and_then(|value| value.strip_suffix(".json"))
        else {
            continue;
        };
        let Ok(terminal_id) = number_text.parse::<i64>() else {
            continue;
        };
        if terminal_id >= min_terminal_id {
            return Ok(true);
        }
    }

    Ok(false)
}

fn cleanup_extra_procedure_files(
    output_dir: &Path,
    allowed_terminal_ids: &FxHashSet<i64>,
) -> Result<usize> {
    let procedure_dir = output_dir.join("ProcedureLegs");
    let mut removed = 0;

    if !procedure_dir.exists() {
        return Ok(0);
    }

    for entry in fs::read_dir(&procedure_dir).with_context(|| {
        format!(
            "failed to read ProcedureLegs directory: {}",
            procedure_dir.display()
        )
    })? {
        let entry = entry.with_context(|| {
            format!(
                "failed to read ProcedureLegs entry: {}",
                procedure_dir.display()
            )
        })?;
        let Ok(file_name) = entry.file_name().into_string() else {
            continue;
        };
        let Some(number_text) = file_name
            .strip_prefix("TermID_")
            .and_then(|v| v.strip_suffix(".json"))
        else {
            continue;
        };
        let Ok(terminal_id) = number_text.parse::<i64>() else {
            continue;
        };
        if !allowed_terminal_ids.contains(&terminal_id) {
            fs::remove_file(entry.path()).with_context(|| {
                format!(
                    "failed to remove stale ProcedureLegs file: {}",
                    entry.path().display()
                )
            })?;
            removed += 1;
        }
    }

    Ok(removed)
}

fn collect_waypoint_reference_ids(terminal_legs: &[TerminalLegRecord]) -> FxHashSet<i64> {
    let mut ids = fast_hash_set();
    for leg in terminal_legs {
        if let Some(id) = leg.wpt_id_num {
            ids.insert(id);
        }
        if let Some(id) = leg.center_id_num {
            ids.insert(id);
        }
    }
    ids
}

fn collect_navaid_reference_ids(terminal_legs: &[TerminalLegRecord]) -> FxHashSet<i64> {
    let mut ids = fast_hash_set();
    for leg in terminal_legs {
        if let Some(id) = leg.nav_id_num {
            ids.insert(id);
        }
    }
    ids
}

fn fetch_waypoint_coordinates_by_db_ids(
    conn: &Connection,
    db_ids: &FxHashSet<i64>,
    waypoint_id_index: &WaypointIdIndex,
) -> Result<FxHashMap<i64, (f64, f64)>> {
    fetch_output_coordinates_by_db_ids(conn, "Waypoints", db_ids, |db_id| {
        waypoint_id_index.output_id_for_db(db_id)
    })
}

fn recover_missing_rf_center_ids(conn: &Connection, legs: &mut [TerminalLegRecord]) -> Result<()> {
    let longitude_col = resolve_longitude_column(conn, "Waypoints")?;
    for leg in legs {
        if leg.track_code != Value::String("RF".to_string()) || leg.center_id_num.is_some() {
            continue;
        }
        let (Some(latitude), Some(longitude)) = (leg.center_lat.as_f64(), leg.center_lon.as_f64())
        else {
            bail!(
                "RF terminal leg {} has no CenterID or usable center coordinates",
                leg.id
            );
        };
        let waypoint_id =
            find_unique_waypoint_id_at_coordinates(conn, &longitude_col, latitude, longitude)?;
        leg.center_id = Value::Number(Number::from(waypoint_id));
        leg.center_id_num = Some(waypoint_id);
    }
    Ok(())
}

fn find_unique_waypoint_id_at_coordinates(
    conn: &Connection,
    longitude_col: &str,
    latitude: f64,
    longitude: f64,
) -> Result<i64> {
    let query = format!(
        "SELECT ID FROM Waypoints WHERE abs(Latitude - ?1) <= 0.000001 AND abs({longitude_col} - ?2) <= 0.000001 ORDER BY ID"
    );
    let mut statement = conn
        .prepare(&query)
        .context("failed to look up RF center waypoint")?;
    let ids = statement
        .query_map([latitude, longitude], |row| row.get::<_, i64>(0))
        .context("failed to query RF center waypoint")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to read RF center waypoint")?;
    match ids.as_slice() {
        [id] => Ok(*id),
        [] => bail!("RF center coordinates do not match a waypoint"),
        _ => bail!("RF center coordinates match multiple waypoints"),
    }
}

fn fetch_navaid_coordinates_by_db_ids(
    conn: &Connection,
    db_ids: &FxHashSet<i64>,
    navaid_id_index: &NavaidIdIndex,
) -> Result<FxHashMap<i64, (f64, f64)>> {
    fetch_output_coordinates_by_db_ids(conn, "Navaids", db_ids, |db_id| {
        navaid_id_index.output_id_for_db(db_id)
    })
}

fn fetch_runway_coordinates_by_ids(
    conn: &Connection,
    ids: &FxHashSet<i64>,
) -> Result<FxHashMap<i64, (f64, f64)>> {
    fetch_coordinates_by_ids(conn, "Runways", ids)
}

fn fetch_coordinates_by_ids(
    conn: &Connection,
    table_name: &str,
    ids: &FxHashSet<i64>,
) -> Result<FxHashMap<i64, (f64, f64)>> {
    if ids.is_empty() {
        return Ok(fast_hash_map());
    }

    let longitude_col = resolve_longitude_column(conn, table_name)?;
    let mut map = fast_hash_map_with_capacity(ids.len());

    let mut id_list: Vec<i64> = ids.iter().copied().collect();
    id_list.sort_unstable();
    for chunk in id_list.chunks(900) {
        let placeholders = std::iter::repeat_n("?", chunk.len())
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "SELECT ID, Latitude, {longitude_col} FROM {table_name} WHERE ID IN ({placeholders})"
        );
        let mut statement = conn
            .prepare(&query)
            .with_context(|| format!("failed to query {table_name} coordinates"))?;
        let rows = statement
            .query_map(params_from_iter(chunk.iter()), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                ))
            })
            .with_context(|| format!("failed to iterate {table_name} coordinates"))?;

        for row in rows {
            let (id, latitude, longitude) =
                row.with_context(|| format!("failed to read {table_name} coordinate row"))?;
            let (Some(latitude), Some(longitude)) = (latitude, longitude) else {
                continue;
            };
            map.insert(id, (latitude, longitude));
        }
    }

    Ok(map)
}

fn fetch_output_coordinates_by_db_ids(
    conn: &Connection,
    table_name: &str,
    db_ids: &FxHashSet<i64>,
    map_id: impl Fn(i64) -> i64,
) -> Result<FxHashMap<i64, (f64, f64)>> {
    let raw_coords = fetch_coordinates_by_ids(conn, table_name, db_ids)?;
    let mut remapped = fast_hash_map_with_capacity(raw_coords.len());
    for (db_id, coords) in raw_coords {
        remapped.entry(map_id(db_id)).or_insert(coords);
    }
    Ok(remapped)
}

fn remap_terminal_leg_reference_ids(
    terminal_legs: &mut [TerminalLegRecord],
    waypoint_id_index: &WaypointIdIndex,
    navaid_id_index: &NavaidIdIndex,
) {
    for leg in terminal_legs {
        leg.remap_reference_ids(waypoint_id_index, navaid_id_index);
    }
}

fn remap_leg_id_value(value: &mut Value, numeric: &mut Option<i64>, id_index: &ReferenceIdIndex) {
    let Some(db_id) = *numeric else {
        return;
    };

    let output_id = id_index.output_id_for_db(db_id);
    if output_id == db_id {
        return;
    }

    *numeric = Some(output_id);
    *value = Value::Number(Number::from(output_id));
}

fn resolve_longitude_column(conn: &Connection, table_name: &str) -> Result<&'static str> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table_name})"))
        .with_context(|| format!("failed to inspect schema for {table_name}"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))
        .with_context(|| format!("failed to iterate schema for {table_name}"))?;

    let mut has_longitude = false;
    let mut has_legacy_longtitude = false;
    for row in rows {
        let name = row.with_context(|| format!("failed to read schema row for {table_name}"))?;
        if name == "Longitude" {
            has_longitude = true;
        }
        if name == "Longtitude" {
            has_legacy_longtitude = true;
        }
    }

    if has_longitude {
        Ok("Longitude")
    } else if has_legacy_longtitude {
        Ok("Longtitude")
    } else {
        anyhow::bail!("{table_name} table has neither Longitude nor Longtitude column")
    }
}

fn value_is_null(value: &Value) -> bool {
    value.is_null()
}

fn should_fill_value(id_value: &Value, lat_value: &Value, lon_value: &Value) -> bool {
    !id_value.is_null() && lat_value.is_null() && lon_value.is_null()
}

fn rounded_number_value(value: f64) -> Value {
    let rounded = (value * 100_000_000.0).round() / 100_000_000.0;
    Number::from_f64(rounded).map_or(Value::Null, Value::Number)
}

fn mark_final_approach_fix(legs: &mut [TerminalLegRecord]) {
    for leg in legs.iter_mut() {
        leg.is_faf = 0;
    }

    // Fenix stores arrival transitions, the primary approach, and the missed
    // approach in one TerminalID.  Never infer a FAF across those boundaries:
    // doing so can mark the last arrival-transition leg as FAF and make the
    // TFDI loader treat the missed approach as a continuation of the wrong
    // branch.  WptDescCode is the source-of-truth marker when it is present.
    let Some(primary_type) = legs.iter().find_map(|leg| match &leg.leg_type {
        Value::String(value) if value != "A" => Some(value.clone()),
        _ => None,
    }) else {
        return;
    };

    let Some(primary_start) = legs
        .iter()
        .position(|leg| matches!(&leg.leg_type, Value::String(value) if value == &primary_type))
    else {
        return;
    };
    let primary_end = legs[primary_start..]
        .iter()
        .position(|leg| leg.is_map == -1)
        .map_or(legs.len(), |offset| primary_start + offset);
    if primary_start >= primary_end {
        return;
    }

    let approach = &mut legs[primary_start..primary_end];
    if let Some(leg) = approach.iter_mut().find(|leg| leg.has_faf_descriptor()) {
        leg.is_faf = -1;
        return;
    }

    // A few Fenix rows have no explicit FAF descriptor.  Keep the fallback
    // inside the primary approach and prefer its first leg; this is safer than
    // the old cross-transition VNAV heuristic and still gives TFDI a stable
    // FAF for those minimal procedures.
    if let Some(first) = approach.first_mut() {
        first.is_faf = -1;
    }
}

impl TerminalLegRecord {
    fn has_faf_descriptor(&self) -> bool {
        matches!(&self.wpt_desc_code, Value::String(value) if value.contains('F'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_test_dir(prefix: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}_{unique}"));
        fs::create_dir_all(dir.join("ProcedureLegs")).expect("create ProcedureLegs");
        dir
    }

    fn test_leg(
        id: i64,
        leg_type: &str,
        descriptor: Option<&str>,
        alt: Option<&str>,
        vnav: Option<f64>,
    ) -> TerminalLegRecord {
        let alt_value = alt.map_or(Value::Null, |value| Value::String(value.to_string()));
        TerminalLegRecord {
            id,
            terminal_id: 1,
            leg_type: Value::String(leg_type.to_string()),
            transition: Value::Null,
            track_code: Value::String("TF".to_string()),
            wpt_id: Value::Null,
            wpt_id_num: None,
            wpt_lat: Value::Null,
            wpt_lon: Value::Null,
            turn_dir: Value::Null,
            nav_id: Value::Null,
            nav_id_num: None,
            nav_lat: Value::Null,
            nav_lon: Value::Null,
            nav_bear: Value::Null,
            nav_dist: Value::Null,
            course: Value::Null,
            distance: Value::Null,
            alt: alt_value.clone(),
            vnav: vnav.map_or(Value::Null, |value| {
                Value::Number(Number::from_f64(value).expect("finite test VNAV"))
            }),
            center_id: Value::Null,
            center_id_num: None,
            center_lat: Value::Null,
            center_lon: Value::Null,
            is_fly_over: Value::Null,
            is_fly_over_num: None,
            speed_limit: Value::Null,
            speed_limit_num: None,
            wpt_desc_code: descriptor.map_or(Value::Null, |value| Value::String(value.to_string())),
            is_faf: 0,
            is_map: if alt_value == Value::String("MAP".to_string()) {
                -1
            } else {
                0
            },
        }
    }

    #[test]
    fn faf_descriptor_does_not_leak_from_arrival_transition_into_primary_approach() {
        let mut legs = vec![
            test_leg(1, "A", Some("E  F"), Some("11000"), None),
            test_leg(2, "A", Some("E  S"), Some("10000"), Some(3.0)),
            test_leg(3, "I", Some("E  I"), Some("09000"), None),
            test_leg(4, "I", Some("E  F"), Some("08000"), Some(3.0)),
            test_leg(5, "I", Some("GY M"), Some("MAP"), Some(3.0)),
            test_leg(6, "I", Some("E  F"), Some("10000"), None),
        ];

        mark_final_approach_fix(&mut legs);

        assert_eq!(legs[0].is_faf, 0);
        assert_eq!(legs[3].is_faf, -1);
        assert_eq!(legs[5].is_faf, 0);
    }

    #[test]
    fn missing_primary_faf_descriptor_falls_back_before_map() {
        let mut legs = vec![
            test_leg(1, "A", Some("E  F"), Some("13600"), None),
            test_leg(2, "A", Some("E  S"), Some("12990"), Some(2.8)),
            test_leg(3, "R", Some("E  S"), Some("11880"), Some(2.8)),
            test_leg(4, "R", Some("EY M"), Some("MAP"), Some(2.8)),
            test_leg(5, "R", Some("E  F"), Some("10000"), None),
        ];

        mark_final_approach_fix(&mut legs);

        assert_eq!(legs[0].is_faf, 0);
        assert_eq!(legs[2].is_faf, -1);
        assert_eq!(legs[4].is_faf, 0);
    }

    #[test]
    fn cleanup_extra_procedure_files_removes_ids_not_in_allowed_set() {
        let output_dir = make_test_dir("fenix2tfdi_cleanup");
        let procedure_dir = output_dir.join("ProcedureLegs");
        let keep_path = procedure_dir.join("TermID_100.json");
        let remove_path = procedure_dir.join("TermID_200.json");
        let ignored_path = procedure_dir.join("not_a_terminal_file.json");

        fs::write(&keep_path, "[]").expect("write keep file");
        fs::write(&remove_path, "[]").expect("write stale file");
        fs::write(&ignored_path, "[]").expect("write ignored file");

        let allowed: FxHashSet<i64> = std::iter::once(100_i64).collect();
        let removed = cleanup_extra_procedure_files(&output_dir, &allowed).expect("cleanup files");

        assert_eq!(removed, 1);
        assert!(keep_path.exists());
        assert!(!remove_path.exists());
        assert!(ignored_path.exists());

        fs::remove_dir_all(&output_dir).expect("remove temp dir");
    }

    #[test]
    fn terminal_leg_type_preserves_the_tfdi_string_enum() {
        let connection = Connection::open_in_memory().unwrap();
        let record = connection
            .query_row(
                "SELECT 1, 2, '5', 'ALL', 'IF', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL",
                [],
                TerminalLegRecord::from_row,
            )
            .unwrap();

        let output = serde_json::to_value(record).unwrap();
        assert_eq!(output["Type"], Value::String("5".to_string()));
    }

    #[test]
    fn rf_center_coordinates_resolve_to_one_waypoint() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute(
                "CREATE TABLE Waypoints (ID INTEGER, Latitude REAL, Longtitude REAL)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO Waypoints VALUES (325995, 27.711278, 101.307444)",
                [],
            )
            .unwrap();

        assert_eq!(
            find_unique_waypoint_id_at_coordinates(
                &connection,
                "Longtitude",
                27.711278,
                101.307444,
            )
            .unwrap(),
            325995
        );
    }

    #[test]
    fn source_terminal_filter_ids_include_new_invalid_zuls_procedures() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute(
                "CREATE TABLE Terminals (ID INTEGER, AirportID INTEGER, ICAO TEXT, Name TEXT, FullName TEXT)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO Terminals VALUES (174768,1,'ZULS','R10L','R10L RNAV 10L'),(180000,NULL,'ZZZZ','ORPHAN','ORPHAN'),(200000,1,'ZBAA','R10L','R10L RNAV 10L')",
                [],
            )
            .unwrap();

        let excluded = fetch_excluded_source_terminal_ids(&connection).unwrap();

        assert_eq!(excluded, [174768, 180000].into_iter().collect());
    }

    #[test]
    fn terminal_leg_selection_uses_output_terminal_ids_instead_of_a_numeric_threshold() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE TerminalLegs (
                    ID INTEGER,
                    TerminalID INTEGER,
                    Type TEXT,
                    Transition TEXT,
                    TrackCode TEXT,
                    WptID INTEGER,
                    WptLat REAL,
                    WptLon REAL,
                    TurnDir TEXT,
                    NavID INTEGER,
                    NavLat REAL,
                    NavLon REAL,
                    NavBear REAL,
                    NavDist REAL,
                    Course REAL,
                    Distance REAL,
                    Alt TEXT,
                    Vnav REAL,
                    CenterID INTEGER,
                    CenterLat REAL,
                    CenterLon REAL,
                    WptDescCode TEXT
                );
                CREATE TABLE TerminalLegsEx (ID INTEGER, IsFlyOver INTEGER, SpeedLimit INTEGER);",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO TerminalLegs (ID, TerminalID, Type, Transition, TrackCode)
                 VALUES (1, 35762, '5', 'RW34', 'CF'), (2, 97105, '5', 'RW34', 'CF')",
                [],
            )
            .unwrap();

        let selected_terminal_ids: FxHashSet<i64> = std::iter::once(35762).collect();
        let terminal_legs = fetch_terminal_legs(&connection, &selected_terminal_ids).unwrap();

        assert_eq!(terminal_legs.len(), 1);
        assert_eq!(terminal_legs[0].terminal_id, 35762);
    }

    #[test]
    fn cleanup_uses_the_final_terminal_id_set() {
        let existing_terminal_ids = [35762, 37038, 37039].into_iter().collect();
        let source_terminal_ids = std::iter::once(35762).collect();
        let excluded_existing_terminal_ids = [35762, 35763].into_iter().collect();

        let allowed = cleanup_allowed_terminal_ids(
            &existing_terminal_ids,
            &source_terminal_ids,
            &excluded_existing_terminal_ids,
        );

        assert_eq!(allowed, [35762, 37038, 37039].into_iter().collect());
    }
}

fn fetch_excluded_source_terminal_ids(conn: &Connection) -> Result<FxHashSet<i64>> {
    let mut statement = conn
        .prepare("SELECT ID, AirportID, ICAO, Name, FullName FROM Terminals")
        .context("failed to query source terminal filters")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .context("failed to iterate source terminal filters")?;

    let mut excluded = fast_hash_set();
    for row in rows {
        let (id, airport_id, icao, name, full_name) =
            row.context("failed to read source terminal filter")?;
        if airport_id.is_none()
            || is_excluded_terminal(icao.as_deref(), name.as_deref(), full_name.as_deref())
        {
            excluded.insert(id);
        }
    }
    Ok(excluded)
}
