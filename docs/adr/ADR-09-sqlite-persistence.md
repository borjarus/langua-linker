# ADR-09: SQLite persistence — worker thread, migrations, idempotency

- Status: Accepted (2026-11-02, owner decision after the Android emulator run of the Phase 7 build — [device-smoke.md](../release/device-smoke.md#phase-7-android-emulator-run-2026-11-02)); proposed 2026-10-30
- Plan references: Phase 7 steps 19–25, 28, 30; integration checklist rules 1–5

## Context

Phase 7 introduces the durable collection under the accepted GUI. The plan requires that SQL never runs on UI or Tokio threads, that writes are serialized, that migrations are numbered, checksummed and recoverable, and that the frontend's `operationId`/`expectedVersion`/job contract is backed by the database.

## Decision

1. **Driver.** `rusqlite` 0.41 with bundled SQLite (identical version and FTS5/JSON1 on every platform), all SQL parameterized.
2. **One connection on a worker thread.** `SqliteDatabase` owns the connection on a dedicated thread; callers send closures and block on the reply. Tauri commands call it from `spawn_blocking`. Reads and writes share the worker; a read-only pool can be added behind the `UnitOfWork` port if profiling asks for it.
3. **Unit of work.** `UnitOfWork::read` runs a deferred transaction, `write` an `IMMEDIATE` one; `Ok` commits, `Err` rolls back. All repositories are reachable from one transaction (`Repositories`), so a use case never commits half.
4. **Connection settings.** `foreign_keys=ON` (verified after setting), WAL + `synchronous=NORMAL`, `busy_timeout` 5 s. SQLite `BUSY/LOCKED` → `DB_BUSY`, `FULL` → `STORAGE_FULL`, anything else → `INTERNAL` (text stays in the log).
5. **Migrations.** `migrations/NNNN_*.sql`, versions 1..N without gaps, each in its own transaction with its bookkeeping row and `PRAGMA user_version`. SHA-256 of every applied migration is checked on open; a changed file refuses to open. Before upgrading an existing file, `VACUUM INTO <name>.pre-v<N>.sqlite` writes a copy — also the disk-space check (nothing is migrated if the copy cannot be written). **No downgrade:** a newer database is left untouched and every data command returns `SCHEMA_TOO_NEW`. Fresh schema starts at v1 (ADR-08).
6. **Enum storage.** Explicit integer codes defined next to each domain enum (`db_code`/`from_db_code`); `CardState`, `QueueState` and flags use Anki's numbers. Never enum order.
7. **Idempotency and versions.** `operations(operation_id, operation, result)` records the first result of a mutation; versioned rows update with `WHERE version = ?` and explain a miss as `NOT_FOUND`, `DELETED` or `VERSION_CONFLICT { currentVersion }`.
8. **Jobs.** The `jobs` table is the truth; every status change is written with a higher `sequence` before its `job_progress` event. Only `cancel_job` cancels; after `begin_commit` a cancel is ignored. Jobs left `queued`/`running` by a killed process are marked failed (`INTERNAL`, `reason: interrupted`) at startup.
9. **DTO source.** Rust DTOs derive `ts_rs::TS` and render `frontend/src/generated/contracts.ts`; a test fails on drift. DTOs move to Rust as their commands become real; integers are `u32`/`f64` so an `i64` cannot silently cross as a JavaScript number (it would render as `bigint` and fail the type check).

## Consequences

- Reads queue behind writes. The due-queue query on 50 000 cards / 500 000 logs takes ≤ 12.7 ms on the development PC (release build, `due_query_budget_on_the_plan_collection`); the device budget (50 ms) is measured in the device run.
- WAL leaves `-wal`/`-shm` files next to the database; backups (Phase 12) must use `VACUUM INTO` or the backup API, never a file copy.
- A process kill is covered by SQLite's atomic commit; tests simulate it with an uncommitted transaction and a reopened file, not with a real kill.
- Android emulator (2026-11-02): WAL on app storage, newer-schema refusal with the file untouched, checksum refusal, interrupted-job recovery and the SecretStore work; the migration tests (safety copy included) pass on device storage; the due query takes 17–28 ms (worst of 20, three runs) against the 50 ms budget.
- Carried over, not blocking: the due-query budget and start time on a physical Android device, and the iOS simulator run.
- The main file stays small until a checkpoint (after a fresh start the schema lives in the `-wal` file), so copying `collection.sqlite` alone loses data — another reason backups use `VACUUM INTO`.
