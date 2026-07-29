# Fenix to TFDI domain context

## Purpose

Generate a testable TFDI MD-11 navigation-data candidate from the supplied 2607 source data and Fenix `nd.db3`, while preserving TFDI-specific runtime contracts from an official `Nav-Primary` template.

## Domain language

- **Raw source**: the files under `2607`, plus the supplied Fenix `nd.db3` while the complete 2607 parser is being built. Raw source data is content, not a target schema.
- **Normalized model**: target-independent airports, runways, fixes, navaids, airways, procedures, holdings, communications, airspace, and MORA with source precision and provenance retained.
- **TFDI adapter**: the only layer allowed to apply TFDI JSON field names, IDs, null rules, lookup layout, procedure filenames, and deployment paths.
- **Official template**: a known-working TFDI `Nav-Primary` directory used to identify the target runtime contract and preserve non-China global data. It is not a navigation-content source.
- **Candidate output**: an isolated generated `Nav-Primary` directory. It is a test build until in-simulator validation succeeds.
- **Deployment**: replacing the simulator's active TFDI data only after checking that the game is closed and creating a timestamped backup.
- **Conversion report**: deterministic counts of inserted, replaced, preserved, rejected, and degraded records plus validation findings.

## Invariants

1. Raw-source parsing must not depend on TFDI field names or IDs.
2. TFDI-specific behavior belongs in the TFDI adapter, not shared model code.
3. China records are replaced by explicit keys; unrelated global template records are preserved.
4. Repeated conversion with identical inputs must produce identical output and must not accumulate duplicates.
5. Candidate generation never writes directly into the active simulator directory.
6. Deployment requires a stopped simulator, timestamped backup, and post-copy hash verification.
7. Official databases, simulator binaries, diagnostics, decompiler output, test packages, and generated navigation data are never committed.

## Verified target contract

See `docs/tfdi-contract.md` for the inspected TFDI runtime, load order, JSON shapes, cycle metadata, and point-check baseline.

