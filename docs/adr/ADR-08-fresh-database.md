# ADR-08: Fresh Rust database, no KMP user-data migration

- Status: Accepted (product owner, 2026-10-12) — resolves D-12
- Plan references: Phase 0 step 4, Phase 7 steps 21–22, R7

## Decision

The Rust application starts with a fresh SQLite database at schema v1. No KMP database, backup or settings are imported or converted. There is no KMP→Rust upgrade path in the installer.

## Consequences

- Phase 7 step 22 (backup → staging → map → validate → swap for existing KMP data) is out of scope. Step 21/23 still apply to Rust's own migrations from v1 onward.
- KMP schema (`*.sq`, `1.sqm`) is kept only as a design reference for the Rust schema, not as a migration source. Rust migration numbering is independent.
- KMP bundled content (manifest + NDJSON), FSRS vectors and tests remain valuable: they are copied as fixtures/reference vectors (see Phase 0 manifest), not as user data.
- Plaintext KMP API keys (if any) are not migrated (Phase 7 step 27 not needed).
- If a KMP build is ever distributed before Rust v1, this ADR must be revisited: shipping a KMP build to users would create data that this decision does not protect.
