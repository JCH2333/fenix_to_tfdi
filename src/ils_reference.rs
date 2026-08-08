//! Conservative ILS procedure backfill from an older Fenix cycle.
//!
//! The reference is never used as a general navdata source. A procedure is
//! accepted only when its airport, runway, localizer identifier, frequency,
//! and localizer course all match the active cycle.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const COORDINATE_TOLERANCE: f64 = 0.000_01;
const COURSE_TOLERANCE: f64 = 0.01;

#[derive(Debug)]
pub struct MergedIlsReferenceDatabase {
    path: PathBuf,
    pub added_procedures: usize,
}

impl MergedIlsReferenceDatabase {
    pub fn create(active_db: &Path, reference_db: &Path) -> Result<Self> {
        if !reference_db.is_file() {
            bail!(
                "ILS reference database does not exist: {}",
                reference_db.display()
            );
        }
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fenix-to-tfdi-ils-reference-{}-{sequence}.db3",
            std::process::id()
        ));
        fs::copy(active_db, &path).with_context(|| {
            format!(
                "failed to create temporary enriched database: {}",
                path.display()
            )
        })?;

        let added_procedures = match merge_missing_chinese_ils_procedures(&path, reference_db) {
            Ok(count) => count,
            Err(error) => {
                let _ = fs::remove_file(&path);
                return Err(error);
            }
        };
        Ok(Self {
            path,
            added_procedures,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for MergedIlsReferenceDatabase {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(self.path.with_extension("db3-wal"));
        let _ = fs::remove_file(self.path.with_extension("db3-shm"));
    }
}

#[derive(Debug)]
struct ReferenceProcedure {
    id: i64,
    icao: String,
    proc: String,
    full_name: String,
    name: String,
    runway: String,
    ils_ident: String,
    ils_frequency: i64,
    ils_course: f64,
}

fn merge_missing_chinese_ils_procedures(active_db: &Path, reference_db: &Path) -> Result<usize> {
    let mut conn = Connection::open(active_db)
        .with_context(|| format!("failed to open temporary database: {}", active_db.display()))?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_tfdi_ils_patch_waypoints ON Waypoints(Ident, Latitude, Longtitude);\
         CREATE INDEX IF NOT EXISTS idx_tfdi_ils_patch_navaids ON Navaids(Ident, Type, Freq, Latitude, Longtitude);\
         CREATE INDEX IF NOT EXISTS idx_tfdi_ils_patch_terminals_by_ils ON Terminals(Proc, IlsID);\
         CREATE INDEX IF NOT EXISTS idx_tfdi_ils_patch_terminals_by_name ON Terminals(ICAO, Name);\
         CREATE INDEX IF NOT EXISTS idx_tfdi_ils_patch_airports ON Airports(ICAO);\
         CREATE INDEX IF NOT EXISTS idx_tfdi_ils_patch_runways ON Runways(AirportID, Ident);\
         CREATE INDEX IF NOT EXISTS idx_tfdi_ils_patch_ilses ON Ilses(RunwayID, Ident, Freq, LocCourse);",
    )
    .context("failed to prepare temporary ILS reference indexes")?;
    conn.execute(
        "ATTACH DATABASE ?1 AS ils_reference",
        [reference_db.to_string_lossy().as_ref()],
    )
    .context("failed to attach ILS reference database")?;
    let transaction = conn
        .transaction()
        .context("failed to start ILS reference merge")?;
    let procedures = load_reference_ils_procedures(&transaction)?;
    let mut ids = IdAllocator::load(&transaction)?;
    let mut added = 0;

    for procedure in procedures {
        let Some((airport_id, runway_id, ils_id)) =
            find_matching_active_ils(&transaction, &procedure)?
        else {
            continue;
        };
        if active_ils_procedure_exists(&transaction, ils_id)? {
            continue;
        }
        if active_terminal_name_exists(&transaction, &procedure.icao, &procedure.name)? {
            continue;
        }

        let terminal_id = ids.next_terminal();
        transaction.execute(
            "INSERT INTO Terminals (ID, AirportID, Proc, ICAO, FullName, Name, Rwy, RwyID, IlsID) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![terminal_id, airport_id, procedure.proc, procedure.icao, procedure.full_name, procedure.name, procedure.runway, runway_id, ils_id],
        ).context("failed to insert reference ILS terminal")?;

        copy_procedure_legs(&transaction, procedure.id, terminal_id, &mut ids)?;
        added += 1;
    }

    transaction
        .commit()
        .context("failed to commit ILS reference merge")?;
    Ok(added)
}

fn load_reference_ils_procedures(conn: &Connection) -> Result<Vec<ReferenceProcedure>> {
    let mut statement = conn
        .prepare(
            "SELECT t.ID, t.ICAO, t.Proc, t.FullName, t.Name, t.Rwy, i.Ident, i.Freq, i.LocCourse \
         FROM ils_reference.Terminals t \
         JOIN ils_reference.Ilses i ON i.ID = t.IlsID \
         WHERE t.Proc = '3' AND t.IlsID IS NOT NULL AND t.ICAO LIKE 'Z%' \
         ORDER BY t.ICAO, t.Rwy, t.Name, t.ID",
        )
        .context("failed to query ILS reference procedures")?;
    statement
        .query_map([], |row| {
            Ok(ReferenceProcedure {
                id: row.get(0)?,
                icao: row.get(1)?,
                proc: row.get(2)?,
                full_name: row.get(3)?,
                name: row.get(4)?,
                runway: row.get(5)?,
                ils_ident: row.get(6)?,
                ils_frequency: row.get(7)?,
                ils_course: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to read ILS reference procedures")
}

fn find_matching_active_ils(
    conn: &Connection,
    procedure: &ReferenceProcedure,
) -> Result<Option<(i64, i64, i64)>> {
    conn.query_row(
        "SELECT ap.ID, rw.ID, ils.ID FROM Ilses ils \
         JOIN Runways rw ON rw.ID = ils.RunwayID \
         JOIN Airports ap ON ap.ID = rw.AirportID \
         WHERE ap.ICAO = ?1 AND rw.Ident = ?2 AND ils.Ident = ?3 AND ils.Freq = ?4 \
         AND abs(ils.LocCourse - ?5) <= ?6",
        params![
            procedure.icao,
            procedure.runway,
            procedure.ils_ident,
            procedure.ils_frequency,
            procedure.ils_course,
            COURSE_TOLERANCE
        ],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
    .context("failed to match active ILS facility")
}

fn active_ils_procedure_exists(conn: &Connection, ils_id: i64) -> Result<bool> {
    conn.query_row(
        "SELECT 1 FROM Terminals WHERE Proc = '3' AND IlsID = ?1 LIMIT 1",
        [ils_id],
        |_| Ok(()),
    )
    .optional()
    .map(|row| row.is_some())
    .context("failed to check active ILS procedure")
}

fn active_terminal_name_exists(conn: &Connection, icao: &str, name: &str) -> Result<bool> {
    conn.query_row(
        "SELECT 1 FROM Terminals WHERE ICAO = ?1 AND Name = ?2 LIMIT 1",
        params![icao, name],
        |_| Ok(()),
    )
    .optional()
    .map(|row| row.is_some())
    .context("failed to check existing terminal name")
}

#[derive(Debug)]
struct IdAllocator {
    terminal: i64,
    terminal_leg: i64,
    waypoint: i64,
    navaid: i64,
}

impl IdAllocator {
    fn load(conn: &Connection) -> Result<Self> {
        Ok(Self {
            terminal: max_id(conn, "Terminals")?,
            terminal_leg: max_id(conn, "TerminalLegs")?,
            waypoint: max_id(conn, "Waypoints")?,
            navaid: max_id(conn, "Navaids")?,
        })
    }
    fn next_terminal(&mut self) -> i64 {
        self.terminal += 1;
        self.terminal
    }
    fn next_terminal_leg(&mut self) -> i64 {
        self.terminal_leg += 1;
        self.terminal_leg
    }
    fn next_waypoint(&mut self) -> i64 {
        self.waypoint += 1;
        self.waypoint
    }
    fn next_navaid(&mut self) -> i64 {
        self.navaid += 1;
        self.navaid
    }
}

fn max_id(conn: &Connection, table: &str) -> Result<i64> {
    conn.query_row(
        &format!("SELECT coalesce(max(ID), 0) FROM {table}"),
        [],
        |row| row.get(0),
    )
    .with_context(|| format!("failed to allocate IDs for {table}"))
}

fn copy_procedure_legs(
    conn: &Connection,
    reference_terminal_id: i64,
    active_terminal_id: i64,
    ids: &mut IdAllocator,
) -> Result<()> {
    let mut statement = conn.prepare(
        "SELECT l.ID, l.Type, l.Transition, l.TrackCode, l.WptID, l.WptLat, l.WptLon, l.TurnDir, \
         l.NavID, l.NavLat, l.NavLon, l.NavBear, l.NavDist, l.Course, l.Distance, l.Alt, l.Vnav, \
         l.CenterID, l.CenterLat, l.CenterLon, l.WptDescCode, ex.IsFlyOver, ex.SpeedLimit \
         FROM ils_reference.TerminalLegs l \
         LEFT JOIN ils_reference.TerminalLegsEx ex ON ex.ID = l.ID \
         WHERE l.TerminalID = ?1 ORDER BY l.ID",
    ).context("failed to query reference ILS legs")?;
    let rows = statement
        .query_map([reference_terminal_id], |row| {
            Ok(ReferenceLeg {
                leg_type: row.get(1)?,
                transition: row.get(2)?,
                track_code: row.get(3)?,
                wpt_id: row.get(4)?,
                wpt_lat: row.get(5)?,
                wpt_lon: row.get(6)?,
                turn_dir: row.get(7)?,
                nav_id: row.get(8)?,
                nav_lat: row.get(9)?,
                nav_lon: row.get(10)?,
                nav_bear: row.get(11)?,
                nav_dist: row.get(12)?,
                course: row.get(13)?,
                distance: row.get(14)?,
                alt: row.get(15)?,
                vnav: row.get(16)?,
                center_id: row.get(17)?,
                center_lat: row.get(18)?,
                center_lon: row.get(19)?,
                wpt_desc_code: row.get(20)?,
                is_fly_over: row.get(21)?,
                speed_limit: row.get(22)?,
            })
        })
        .context("failed to iterate reference ILS legs")?;
    for row in rows {
        let leg = row.context("failed to read reference ILS leg")?;
        let wpt_id = leg
            .wpt_id
            .map(|id| ensure_waypoint(conn, id, ids))
            .transpose()?;
        let nav_id = leg
            .nav_id
            .map(|id| ensure_navaid(conn, id, ids))
            .transpose()?;
        let center_id = leg
            .center_id
            .map(|id| ensure_waypoint(conn, id, ids))
            .transpose()?;
        let leg_id = ids.next_terminal_leg();
        // TerminalLegs has a reverse foreign key to its extension row.
        conn.execute(
            "INSERT INTO TerminalLegsEx (ID, IsFlyOver, SpeedLimit) VALUES (?1, ?2, ?3)",
            params![leg_id, leg.is_fly_over, leg.speed_limit],
        )
        .context("failed to insert reference ILS leg extension")?;
        conn.execute(
            "INSERT INTO TerminalLegs (ID, TerminalID, Type, Transition, TrackCode, WptID, WptLat, WptLon, TurnDir, NavID, NavLat, NavLon, NavBear, NavDist, Course, Distance, Alt, Vnav, CenterID, CenterLat, CenterLon, WptDescCode) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
            params![leg_id, active_terminal_id, leg.leg_type, leg.transition, leg.track_code, wpt_id, leg.wpt_lat, leg.wpt_lon, leg.turn_dir, nav_id, leg.nav_lat, leg.nav_lon, leg.nav_bear, leg.nav_dist, leg.course, leg.distance, leg.alt, leg.vnav, center_id, leg.center_lat, leg.center_lon, leg.wpt_desc_code],
        ).context("failed to insert reference ILS leg")?;
    }
    Ok(())
}

struct ReferenceLeg {
    leg_type: String,
    transition: Option<String>,
    track_code: String,
    wpt_id: Option<i64>,
    wpt_lat: Option<f64>,
    wpt_lon: Option<f64>,
    turn_dir: Option<String>,
    nav_id: Option<i64>,
    nav_lat: Option<f64>,
    nav_lon: Option<f64>,
    nav_bear: Option<f64>,
    nav_dist: Option<f64>,
    course: Option<f64>,
    distance: Option<f64>,
    alt: Option<String>,
    vnav: Option<f64>,
    center_id: Option<i64>,
    center_lat: Option<f64>,
    center_lon: Option<f64>,
    wpt_desc_code: Option<String>,
    is_fly_over: Option<i64>,
    speed_limit: Option<f64>,
}

fn ensure_waypoint(conn: &Connection, reference_id: i64, ids: &mut IdAllocator) -> Result<i64> {
    let row = conn.query_row(
        "SELECT Ident, Collocated, Name, Latitude, Longtitude, NavaidID FROM ils_reference.Waypoints WHERE ID = ?1",
        [reference_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, f64>(3)?, row.get::<_, f64>(4)?, row.get::<_, Option<i64>>(5)?)),
    ).optional().context("failed to read reference waypoint")?.context("reference procedure points to missing waypoint")?;
    let (ident, collocated, name, latitude, longitude, navaid_id) = row;
    let matches = matching_ids(conn, "Waypoints", "Ident", &ident, latitude, longitude)?;
    if let [id] = matches.as_slice() {
        return Ok(*id);
    }
    if matches.len() > 1 {
        bail!("reference waypoint {ident} has ambiguous active-cycle coordinate match");
    }
    let new_id = ids.next_waypoint();
    let mapped_navaid = navaid_id
        .map(|id| ensure_navaid(conn, id, ids))
        .transpose()?;
    conn.execute("INSERT INTO Waypoints (ID, Ident, Collocated, Name, Latitude, Longtitude, NavaidID) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![new_id, ident, collocated, name, latitude, longitude, mapped_navaid])
        .context("failed to insert reference waypoint")?;
    if let Some(country) = conn
        .query_row(
            "SELECT Country FROM ils_reference.WaypointLookup WHERE ID = ?1 LIMIT 1",
            [reference_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        conn.execute(
            "INSERT INTO WaypointLookup (Ident, Country, ID) VALUES (?1, ?2, ?3)",
            params![ident, country, new_id],
        )
        .context("failed to insert reference waypoint lookup")?;
    }
    Ok(new_id)
}

fn ensure_navaid(conn: &Connection, reference_id: i64, ids: &mut IdAllocator) -> Result<i64> {
    let row = conn.query_row(
        "SELECT Ident, Type, Name, Freq, Channel, Usage, Latitude, Longtitude, Elevation, SlavedVar, MagneticVariation, Range FROM ils_reference.Navaids WHERE ID = ?1",
        [reference_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, f64>(6)?, r.get::<_, f64>(7)?, r.get::<_, Option<i64>>(8)?, r.get::<_, Option<f64>>(9)?, r.get::<_, Option<f64>>(10)?, r.get::<_, Option<i64>>(11)?)),
    ).optional().context("failed to read reference navaid")?.context("reference procedure points to missing navaid")?;
    let (
        ident,
        nav_type,
        name,
        frequency,
        channel,
        usage,
        latitude,
        longitude,
        elevation,
        slaved_var,
        magnetic_variation,
        range,
    ) = row;
    let mut statement = conn.prepare("SELECT ID FROM Navaids WHERE Ident = ?1 AND Type = ?2 AND Freq = ?3 AND abs(Latitude - ?4) <= ?5 AND abs(Longtitude - ?6) <= ?5 ORDER BY ID")?;
    let matches = statement
        .query_map(
            params![
                ident,
                nav_type,
                frequency,
                latitude,
                COORDINATE_TOLERANCE,
                longitude
            ],
            |r| r.get::<_, i64>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if let [id] = matches.as_slice() {
        return Ok(*id);
    }
    if matches.len() > 1 {
        bail!("reference navaid {ident} has ambiguous active-cycle coordinate match");
    }
    let new_id = ids.next_navaid();
    conn.execute("INSERT INTO Navaids (ID, Ident, Type, Name, Freq, Channel, Usage, Latitude, Longtitude, Elevation, SlavedVar, MagneticVariation, Range) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)", params![new_id, ident, nav_type, name, frequency, channel, usage, latitude, longitude, elevation, slaved_var, magnetic_variation, range])?;
    if let Some((country, key_code)) = conn
        .query_row(
            "SELECT Country, NavKeyCode FROM ils_reference.NavaidLookup WHERE ID = ?1 LIMIT 1",
            [reference_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()?
    {
        conn.execute("INSERT OR IGNORE INTO NavaidLookup (Ident, Type, Country, NavKeyCode, ID) VALUES (?1, ?2, ?3, ?4, ?5)", params![ident, nav_type, country, key_code, new_id])?;
    }
    Ok(new_id)
}

fn matching_ids(
    conn: &Connection,
    table: &str,
    ident_column: &str,
    ident: &str,
    latitude: f64,
    longitude: f64,
) -> Result<Vec<i64>> {
    let sql = format!(
        "SELECT ID FROM {table} WHERE {ident_column} = ?1 AND abs(Latitude - ?2) <= ?3 AND abs(Longtitude - ?4) <= ?3 ORDER BY ID"
    );
    let mut statement = conn.prepare(&sql)?;
    statement
        .query_map(
            params![ident, latitude, COORDINATE_TOLERANCE, longitude],
            |row| row.get(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to match reference waypoint")
}
