# ADR-21: Release engineering, compliance gates and incident response

- Status: Proposed (2026-11-17); engineering implemented in Phase 17. Acceptance after the first store submission (Play internal testing + TestFlight) passes review with these settings, and the owner/legal items in [compliance.md](../release/compliance.md) are closed.
- Plan references: Phase 17 steps 1–19 and DoD; D-01 (mobile only), D-02 (BYOK, no proxy), D-03 (no payments), D-07 (audience/age rating), D-08 (telemetry vendor, open); ADR-13, ADR-14, ADR-17, ADR-18, ADR-20

## Context

Phases 1–16 delivered the v1 scope with desktop/emulator evidence and a Galaxy A56 run. Releases so far were GitHub pre-releases with a sideloadable APK. A store release adds requirements the code has to meet (target API level, permissions, version codes, symbols, privacy-policy link, licences) and requirements outside the code (forms, legal review, beta testers, rollout decisions). This ADR fixes the engineering side and the rules the runbook relies on.

## Decision

### Versions and builds (steps 8, 9, 12)

1. **One version** in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` (+ `Cargo.lock`) and `frontend/package.json`; format `X.Y.Z` or `X.Y.Z-(alpha|beta|rc).N`.
2. **Android versionCode** `MMmmppss` = major·10⁶ + minor·10⁴ + patch·100 + suffix (alpha.N → N, beta.N → 30+N, rc.N → 60+N with N = 1–29, final → 99), written to `bundle.android.versionCode`. Tauri's default (major·10⁶ + minor·10³ + patch) ignores the pre-release, so `0.4.0-beta.2` would repeat the code of `0.4.0-beta.1` and Play would refuse the upload. The new codes continue above every code published so far (`0.4.0-beta.1` was 4000; now 40031).
3. **MSI version** (desktop, unchanged rule): `X.Y.Z.N` for a pre-release, none for a final release.
4. **`scripts/release-check.mjs`** enforces 1–3, target SDK ≥ the Play requirement, and the Android manifest (only reviewed permissions — today `INTERNET`; plugin permissions such as `POST_NOTIFICATIONS` come merged from the notification plugin and are declared in Data Safety — and no Android TV launcher). With `--tag vX` it also requires the matching tag, a `CHANGELOG.md` entry and an https `VITE_PRIVACY_POLICY_URL`. Runs in CI on every push and as the first job of a release.
5. **Android signing**: upload key from CI secrets, Play App Signing holds the app signing key (owner enrols on the first upload). The keystore never enters the repository. iOS: signing and upload from Xcode on the owner's Mac (no Apple secrets in CI until an iOS release job is justified); the Tauri iOS project is archived with the release variant.
6. **Store artifacts**: the release workflow builds the universal APK (GitHub release, sideloading) and the AAB (Play). The AAB carries native symbol tables (`ndk.debugSymbolLevel = SYMBOL_TABLE`) so Play Console symbolicates native crashes; the R8 mapping is kept as a 90-day workflow artifact. No source maps ship in the WebView bundle (`verify-release-bundle.mjs` fails on `.map` files and `sourceMappingURL`); there is no crash vendor yet (D-08), so maps are not uploaded anywhere either. iOS dSYMs stay in the Xcode archive.
7. **Provenance**: lockfiles are used with `--locked` / `npm ci`; the publish job writes `SHA256SUMS` and a GitHub build-provenance attestation for every attached package. Release notes are the `CHANGELOG.md` entry.
8. **No desktop updater** (step 11): desktop is not a release target (D-01); updates come from the stores.

### Android platform level (step 7)

9. `compileSdk`/`targetSdk` 36 (Play: new apps and updates since 2026-08-31). Targeting 35+ enforces edge-to-edge and removes keyboard `adjustResize`; `MainActivity` pads the content view by the system bars, display cutout and IME insets, so the WebView keeps its previous layout, and the theme gives the transparent bars the app background and matching icon contrast (light/dark follow the system theme). AGP 8.5 builds against API 36 with `android.suppressUnsupportedCompileSdk=36`; an AGP upgrade is separate maintenance. Device check required before submission (runbook checklist).

### Compliance in the app (steps 1, 5, 6)

10. **Licences**: `scripts/licenses.mjs` generates `src-tauri/third-party-licenses.json` from the shipped graph (Rust normal dependencies of the shell for the mobile targets, production npm packages, Gradle libraries, bundled SQLite) and fails on any licence expression that cannot be satisfied with the allowlist (permissive licences, MPL-2.0 for unmodified crates, CDLA-Permissive-2.0, Unicode-3.0, public domain). CI fails when the committed report is stale. The Licences screen shows the whole report.
11. **Privacy policy**: hosted outside the app (URL per build, `VITE_PRIVACY_POLICY_URL`, https only); linked from Privacy and About. The Privacy screen states what leaves the device. Draft text in [privacy-policy.md](../release/privacy-policy.md).
12. **External links** never load inside the WebView: a navigation guard (`src-tauri/src/links.rs`) keeps app URLs, opens other `https` pages in the system browser (`tauri-plugin-opener`, Rust API only — no opener permission is granted to the WebView) and drops every other scheme — except a `mailto:` to the association report address (`REPORT_EMAIL`; one recipient, only `subject`/`body`), which opens the mail app. Before, a link on the Licences screen replaced the app on Android and did nothing on iOS.
13. **Association reports** (Play AI-generated content policy): *Report* on generated and bundled associations; the dialog shows reason, comment and the full message, then opens the mail app addressed to `borjarus@gmail.com` (owner decision 2026-11-17). No server and no new traffic from the app; if store review requires reporting without leaving the app, a report endpoint replaces the mail step.
14. **No account**, therefore no account deletion. "Delete everything" (ADR-17) is the local data deletion; uninstalling removes the rest. Backups and exports are the user's copy outside the app.

### Incident response (steps 16–18)

15. **No remote kill switch** in v1: there is no server (D-02 BYOK, D-08 open), and adding a remote config fetch would be new network traffic to declare. Levers are: halt the staged rollout, the AI on/off and provider settings the user controls, provider-side key revocation by the user, and a hotfix. AI failures are already bounded (3 provider calls per request, back-off up to 15 min, 100 calls/day) and never block offline study (ADR-18).
16. **Database direction**: migrations are forward-only and an older build refuses a newer database without touching it (`newer_database_is_refused_and_untouched`). A bad release is fixed **forward** (a new build with a corrective migration); a rollback means "restore the pre-update automatic/safety backup in the fixed build", never "install the older APK". A migration that changes data must ship with a safety backup (as restore and reset already do) — release checklist item.

## Consequences

- Every version bump touches `tauri.conf.json` (`version`, `bundle.android.versionCode`, Windows `wix.version`), `src-tauri/Cargo.toml`, `Cargo.lock`, `frontend/package.json` and `CHANGELOG.md`; the gate names whatever is missing.
- A tag without `vars.PRIVACY_POLICY_URL` fails — including GitHub-only pre-releases. Intended: every tagged build can go to a store.
- New dependencies regenerate the licence report (`node scripts/licenses.mjs`) in the same commit.
- Store forms and legal text are owner work; [compliance.md](../release/compliance.md) records the evidence they must be consistent with.
