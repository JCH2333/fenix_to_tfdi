# TFDI MD-11 navigation data contract

Status: locally inspected on 2026-07-29. Conversion output remains a test build until in-simulator validation is complete.

## Runtime inspected

- MSFS 2024 WASM root: `%APPDATA%\Microsoft Flight Simulator 2024\WASM\MSFS2024\tfdidesign-aircraft-md11`
- Runtime cache module: `m663c42dcb7b68d27_0.dll`
- Official data template: `work\Nav-Primary`

The runtime module was inspected read-only. No runtime binary, extracted strings, decompiler output, or official navigation data is stored in this repository.

## Confirmed load order

The runtime string table identifies these loaders and paths in order:

1. `Config.json`
2. `Airports.json`
3. `Runways.json`
4. `Terminals.json`
5. `Navaids.json`
6. `NavaidLookup.json`
7. `Waypoints.json`
8. `WaypointLookup.json`
9. `Airways.json`
10. `AirwayLegs.json`
11. `ILSes.json`
12. `ProcedureLegs\TermID_<ID>.json`

TFDI uses cereal JSON deserialization into dedicated loader classes. It is not a DFDv2 SQLite contract.

## Official JSON shapes

| File | Rows in inspected template | Ordered fields | Nullable fields |
| --- | ---: | --- | --- |
| `Config.json` | 3 | `key`, `val` | none |
| `Airports.json` | 17,158 | `ID`, `Name`, `ICAO`, `PrimaryID`, `Latitude`, `Longitude`, `Elevation`, `TransAlt` | `PrimaryID`, `TransAlt` |
| `Runways.json` | 42,528 | `ID`, `AirportID`, `Ident`, `TrueHeading`, `Length`, `Width`, `Surface`, `Latitude`, `Longitude`, `Elevation` | none |
| `Terminals.json` | 97,127 | `ID`, `AirportID`, `Proc`, `ICAO`, `FullName`, `Name`, `Rwy`, `RwyID` | `RwyID` |
| `Navaids.json` | 11,395 | `ID`, `Ident`, `Type`, `Name`, `Freq`, `Channel`, `Usage`, `Latitude`, `Longitude`, `Elevation`, `SlavedVar` | none |
| `NavaidLookup.json` | 11,395 | `Ident`, `Type`, `Country`, `NavKeyCode`, `ID` | none |
| `Waypoints.json` | 321,417 | `ID`, `Ident`, `Name`, `Latitude`, `NavaidID`, `Longitude`, `Collocated` | `NavaidID` |
| `WaypointLookup.json` | 321,417 | `Ident`, `Country`, `ID` | none |
| `Airways.json` | 9,855 | `ID`, `Ident` | none |
| `AirwayLegs.json` | 158,908 | `ID`, `AirwayID`, `Level`, `Waypoint1ID`, `Waypoint2ID`, `IsStart`, `IsEnd`, `Waypoint1`, `Waypoint2` | none |
| `ILSes.json` | 4,402 | `ID`, `RunwayID`, `Freq`, `GsAngle`, `Latitude`, `Longitude`, `Category`, `Ident`, `LocCourse`, `CrossingHeight`, `Elevation`, `HasDme` | none |

Field order is preserved by the adapter even though JSON object order is not semantically significant. This keeps output deterministic and makes comparison with official files practical.

## Cycle metadata

The inspected official files use:

- `Config.json`: `CycleStartDate`, `CycleEndDate`, `CycleName`
- `cycle.json`: `cycle`, `revision`, `name`

For the 2607 template, `Config.json` contains cycle name `2607`, while the supplied Fenix database contains `2607n2`. The adapter must normalize the public cycle identifier and preserve revision separately; copying the old TFDI metadata unchanged is invalid.

## Baseline point checks

The official template contains `ZUUU` but does not contain `ZBCF` or `ZUNZ`. The supplied Fenix database contains all three. Validation must therefore cover:

- `ZBCF`: new airport insertion and references.
- `ZUNZ`: high-elevation airport, transition altitude, runways, and procedures.
- `ZUUU`: replacement of an existing airport without duplication.

## RF procedure legs

TFDI's official 2608 data has a valid `CenterID` on every `TrackCode="RF"` leg.
Fenix 2608 has two ZUXC `MYD36A` RF legs with center coordinates but no center ID;
the converter resolves the coordinates to the unique Fenix waypoint before remapping IDs.
Candidates reject any RF leg without a valid center waypoint reference. This rule is
covered by the terminal-leg coordinate lookup test and the TFDI candidate validator test.

## Procedure-file exception

The official 2608 no-NAIP `Nav-Primary` baseline was checked on 2026-08-15.
Among its 97,104 terminal rows, only terminal `97104` has no
`ProcedureLegs\TermID_97104.json`: `ZYYJ`, `Q09 NDBDME 09`. The converter
preserves this verified template exception. Every other terminal in a candidate
must have a matching procedure file, and the validator rejects missing files.

When source and template terminal IDs collide, procedure-file cleanup uses the
final terminal set: retained template terminals plus source terminals selected
for output. A source terminal rejected by a target-specific filter must not
delete an unrelated retained template procedure that happens to share its ID.

The terminal ID itself is not a safe merge key. For the China-only source
patch, a source terminal whose ID collides with a semantically unrelated
retained template terminal receives a deterministic unused output ID; its
`ProcedureLegs` file name and every leg `TerminalID` are remapped to that same
ID. The retained template terminal and its procedure file remain unchanged.
The adapter only selects source terminals whose ICAO starts with `Z`, so it
cannot duplicate unrelated global template procedures. This was verified with
Fenix 2608 `fenix2` at `ZWAT`: all eight SID and eight STAR records were
previously suppressed by collisions with retained European template IDs; the
fixed candidate contains all 21 ZWAT procedures and still retains
`EGJJ / LELN1J / ID 37394`. Regression coverage:
`source_terminal_id_map_remaps_rows_colliding_with_unrelated_template_ids` and
`source_terminal_id_map_ignores_non_chinese_source_rows`.

## Current upstream gaps

The upstream converter used as the initial code base has these confirmed engineering gaps:

- accepts no command-line arguments;
- requires an auto-detected installed `Nav-Primary` directory;
- writes directly into the installed aircraft data;
- does not create a timestamped backup;
- does not update `Config.json` or `cycle.json` from the selected source cycle;
- has no conversion report or target-contract validator;
- has no separate normalized source-model and TFDI-adapter boundary.
