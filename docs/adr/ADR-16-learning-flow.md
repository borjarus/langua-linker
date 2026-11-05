# ADR-16: Real learning flow on the existing screens

- Status: Proposed (2026-11-05); implemented in Phase 10. Acceptance after the Phase 10 device run (Android, offline).
- Plan references: Phase 10 steps 1–11 and DoD; §4.3 (session state machine); §14.2 (durable undo); ADR-15 (scheduler)

## Context

Phase 9 made scheduling correct in Rust (ADR-15) but nothing in the app used it: the study screens still ran on the demo adapter, the dashboard counted stored card states without daily limits, the streak used UTC day numbers, and `resumable` was always `null`. Phase 10 connects Dashboard → Study → Summary to the real backend without changing the screens (the contract v1 shapes stay as frozen at the Phase 6 gate).

## Decision

### Adapter and contract

1. **No screen changes.** The production root already builds `TauriAppApi`; it calls the Tauri commands generically. Phase 10 adds eleven real commands in `src-tauri/src/study.rs`: `start_study`, `resume_study`, `update_study_session`, `get_rating_preview`, `rate_card`, `undo_review`, `apply_card_action`, `get_session_summary`, `get_note_content`, `set_favorite_association`, `resolve_word_link`.
2. **Rust owns the study DTOs.** `StudyCardDto`, `StudySessionDto`, `RatingPreviewDto`, `ReviewResultDto`, `SessionSummaryDto`, the card-action, word-link, association and tip DTOs and their enums are generated into `frontend/src/generated/contracts.ts`; `types.ts` re-exports them. `Record<Rating, …>` becomes a struct with `Again/Hard/Good/Easy` fields (same JSON). The frozen v1 samples of all eleven operations round-trip through the Rust DTOs (`crates/contracts/tests/contract_v1.rs`), so the version stays 1.2.
3. **Signals after commit.** A committed rating, undo, card action, favorite or link choice emits `collection_changed` (cards/notes/decks/stats as relevant); a retried operation that changed nothing does not. The UI already invalidates its queries on the event and re-reads through AppApi.

### Durable session

4. **The session is the database row.** Queue position is not stored as a list: the queue is rebuilt from the cards on every read, and `study_sessions.current_card_id` keeps the card on screen. `resumeStudy(sessionId)` opens the session by id: a paused session stays paused (the screen offers *Continue*), the current card is kept while it is still due, expired burials are lifted first. After a kill the dashboard offers the open session and the study screen reopens it at the same card.
5. **Ratings commit one by one.** Each rating is one transaction (ADR-15 §15). A request that committed but whose answer never reached the UI is retried with the same `operationId` and returns the first result (`replayed`), never a second log. A request that did not commit leaves nothing.
6. **Answer time** is measured by the UI on a monotonic clock, excluding pauses and time in the background, and capped by the backend at 60 s per rating; `reviewed_at` comes from the backend `Clock`.

### Dashboard counts and streak

7. **Counts are the scheduler's queue.** Each deck's new/learning/review/overdue counts are what a session over that deck (with subdecks) would show today: daily limits over the tree, done-today from valid logs, sibling dedupe. `total` and progress still come from stored card states. "Due today" is the all-decks queue (reviews + learning). Reading the dashboard first returns expired buried cards to their queues, so it runs in a write transaction.
8. **Streak.** The number of consecutive study days with at least one valid (not undone) rating, ending today or yesterday, in the collection calendar (IANA zone + cutoff, days of 23/25 hours across DST). Today without a rating does not break the streak until the day ends. v1 has **no protection days** (freezes); an undone rating no longer counts. The tooltip text on the dashboard explains this.
9. **First completed session** (the summary's reminder offer) is the session of the oldest valid rating made in a session, once that session is finished.

### Card actions and content during study

10. **Actions** take the card version the UI showed; a stale version is `VERSION_CONFLICT`. Suspend/bury/unsuspend/unbury put the card in its natural queue (day-learning when its step is on a later day); user burial lasts until the end of the study day. Deleting the last card of a note deletes the note (soft delete) and turns links to it into `unresolved`; deleting one card of several keeps the note. A flag bumps the card version, so the screen refreshes the preview token.
11. **Favorites and links.** Setting a favorite keeps one favorite per note (bundled associations included). Choosing a word for a link updates that link in place (its id stays, resolution `userOverridden`). Candidates for ambiguous links stay empty until the word search of Phase 11; bundled links are resolved by GUID and are never ambiguous.
12. **Sections follow `DeckCapabilities`.** The backend sends associations, tips and links only for deck types that show them.
13. **Edit before Phase 11.** *Edit* still opens the existing editor; its data command answers `NOT_IMPLEMENTED`, which the editor shows as "not available yet". Nothing is saved by a mock in the real database.

## Consequences

- The study flow works offline end to end on real data; AI generation (Phase 13) and settings (Phase 12) answer `NOT_IMPLEMENTED` and the study screen degrades (TTS stays on, association generation shows an error) without blocking ratings.
- Some commands can return codes the v1 catalogue does not list for them (`VALIDATION_FAILED` for a paused or finished session or a card outside the session, `DELETED` for a deleted deck or note). The UI handles every code through `errors.<CODE>`; the catalogue is corrected with the next minor version instead of changing it now.
- Rebuilding the queue on each read costs about 9 ms for all decks on the 50 000-card seed (development PC); the device measurement is part of the Phase 10 device run.
- A user's link choice can be overwritten by a later bundled content update that relinks the sentence; content overrides for links arrive with Phase 11 editing.
