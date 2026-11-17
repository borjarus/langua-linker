# Compliance evidence for store submission (Phase 17 step 1)

Status 2026-11-17: **engineering evidence complete; every answer below is a draft for the owner and a legal reviewer.** Nothing here is legal advice. The store forms must be filled from this page and from the build actually submitted — if the code changes what leaves the device, this page changes in the same commit.

Decisions it rests on: D-01 mobile v1 (Android + iOS), D-02 BYOK without proxy, D-03 no payments, D-07 adults / not child-directed, D-08 telemetry vendor **open** (nothing is sent), D-14 backup encryption separate. Engineering rules: [ADR-21](../adr/ADR-21-release-engineering.md).

## 1. What leaves the device (network inventory)

Verified in code; the WebView itself cannot reach the network (CSP `connect-src ipc: http://ipc.localhost`, release surface test `release_security_surface_is_minimal`).

| Traffic | When | Data sent | Recipient | Code |
|---|---|---|---|---|
| AI association request | only after AI consent, a provider and the user's own API key are set, and the user asks for an association (or a queued request is retried) | front and back of that one card, the variant, L1/L2 language codes, the fixed prompt; the user's API key as the provider's auth header | the provider the user picked: Anthropic (`api.anthropic.com`), Google Gemini (`generativelanguage.googleapis.com`) or an OpenAI-compatible https endpoint the user typed | `crates/infrastructure/src/llm.rs`, ADR-18 |
| External link | user taps a link (Licences, privacy policy) | the URL | opened in the system browser, not by the app | `src-tauri/src/links.rs` |

Not sent: usage metrics and crash reports (stored locally only, opt-in, ADR-13 — "this version sends nothing"), logs, backups, exports (saved/shared only by the user through the system share sheet or file picker), notifications (scheduled locally by the OS). No analytics or advertising SDK, no account, no remote config.

**Subprocessors / transfers.** The developer runs no server and receives no user data. Under BYOK the AI provider is the user's own vendor, used with the user's own key and account; its terms (training, retention, location — e.g. US transfers for Anthropic/Google) apply between the user and that provider. The privacy policy names the three options and links to their policies. **Owner/legal:** confirm this controller/processor characterisation; confirm the providers' API terms say API inputs are not used for training by default and their retention periods (step 3: "confirmed by terms, not a prompt") and record the dates checked here.

## 2. Google Play Data safety (draft answers)

| Question | Draft answer | Why |
|---|---|---|
| Does the app collect or share user data? | **Yes** | card text is transmitted off the device to the AI provider when the user asks |
| Data type | App activity → *Other user-generated content* (card front/back) | the only user data that leaves the device |
| Collected / shared | Collected: yes. Shared: no — transfer initiated by the user to a service the user chose is not "sharing" under Play's definition; legal to confirm | Play's user-initiated-transfer exception |
| Processed ephemerally | Yes (the app does not store it on any server; the provider's retention is the provider's) | |
| Required or optional | Optional (AI associations can be off; study works without them) | |
| Purpose | App functionality | |
| Encrypted in transit | Yes (HTTPS only, plain http refused) | `settings.rs` validation, `llm.rs` |
| Users can request deletion | Data is on the device: "Delete everything" in Settings → Privacy, or uninstall. No account | ADR-17 |
| Usage metrics / crash data | **Not collected** in this version (local only). Must change to *App info and performance → Crash logs, Diagnostics* and *App activity → App interactions* the day an uploader ships (D-08) | ADR-13 |
| Permissions declared | `INTERNET`; from the notification plugin: `POST_NOTIFICATIONS`, `RECEIVE_BOOT_COMPLETED`, `WAKE_LOCK`. No exact alarm, no location/contacts/storage permission | merged manifest; `release-check.mjs` |

Also on the Play form: *App access* — no login; *Ads* — no; *Target audience* — 18+ (D-07, see §5); *News app* — no; *Government app* — no; *Financial features* — none; *Health* — none.

## 3. Apple App Privacy (draft)

Draft: **"Data Not Collected"**, because neither the developer nor a partner of the developer receives data — the AI request goes from the device to a provider the user contracted with, using the user's key. If legal review reads the BYOK request as collection by a third-party partner, declare *User Content → Other User Content*, not linked to identity, not used for tracking, purpose *App Functionality*. Privacy manifest (`PrivacyInfo.xcprivacy`): no tracking domains; required-reason APIs used by the WebView/Tauri runtime to be listed from the Xcode privacy report of the archived build — **open, do on the Mac when archiving**.

## 4. AI duties and content moderation (step 4)

- **AI label**: every generated association is marked "AI-generated" in the UI; bundled associations are candidates for native review (content/PROVENANCE.md).
- **Moderation in place**: local term list on outgoing card text (never sent when hit) and on every answer (blocked answers are never stored or cached), strict JSON answer ≤ 400 chars, prompts versioned (ADR-18).
- **Reporting — release blocker for Google Play**: Play's AI-generated content policy expects an in-app way to report or flag offensive AI output to the developer. Today the user can only delete an association. A report action (Phase 13 open item: preview of what would be sent, contract minor) needs a destination; with no server (D-02/D-08) the options are a pre-filled e-mail via the share sheet, or a minimal report endpoint. **Owner decision needed before the Play submission.** Apple has no equivalent rule for BYOK text, but the same action would serve both.
- **EU AI Act Art. 50** (transparency for AI-generated text, applies from 2026-08-02): the label covers "users are informed"; legal to confirm no further marking duty applies to short private mnemonic text not published to the public.

## 5. Age rating (step 4)

Do not reuse the old 12+/PEGI 3. Fill the IARC (Play) and Apple questionnaires from the real build: no violence, sex, gambling, drugs, purchases, location sharing or user-to-user communication; **unrestricted web access: no** (links open the system browser); **user-generated content / AI generated text: yes** (associations from a third-party model, filtered by a term list but not human-reviewed). The AI answer is what usually moves the rating; expect 12+ on Apple ("infrequent/mild mature themes" risk from unfiltered model text) and record the questionnaire answers and resulting ratings here with the date. D-07: adults, not child-directed — do not opt into "Designed for Families" or the Kids category.

## 6. Licences, attribution, trademarks (step 5)

- **Third-party software**: `src-tauri/third-party-licenses.json`, generated by `node scripts/licenses.mjs` from the shipped graph (Rust crates for the mobile targets, production npm packages, Gradle libraries, SQLite) — 367 components on 2026-11-17, all on the allowlist (permissive; MPL-2.0 only for unmodified `cssparser`, `selectors`, `dtoa-short`, `option-ext`; CDLA-Permissive-2.0 for `webpki-roots`). Shown in full on the Licences screen; CI fails when stale or when a licence is not allowed. **Legal to confirm** that name + licence + link satisfies the notice requirements (MIT/BSD/Apache-2.0 ask for the copyright notice and licence text; if required, bundle full texts with `cargo about` and the npm `LICENSE` files in a later step). Transitive Gradle/AndroidX libraries are summarised by their top-level coordinates.
- **Bundled content**: release-blocking until D-04 is closed — `content-lint --release` refuses the decks while `rightsConfirmed` is false and the native review is missing ([content/PROVENANCE.md](../../content/PROVENANCE.md)). The beta can ship the candidate decks only if the owner records that decision for the beta channel.
- **Trademarks**: "Anki" appears only as a file format name (`.apkg` export, CSV/Anki interop) — say "compatible with Anki® packages" style wording at most, no logo, no implied endorsement; "FSRS" is an open algorithm name. Store texts in [store-listing.md](store-listing.md) follow this. A trademark search for "LanguaLinker" in the EU/PL/US classes 9/41 is owner work.

## 7. Account and data deletion (step 6)

No account in v1, so Play's account-deletion requirement does not apply (declare "no account creation" on the form). Data control: Settings → Privacy → Delete everything (needs a fresh verified backup or the typed phrase; removes the AI key; keeps backups the user saved), Reset progress, opt-out of metrics (deletes them), AI consent withdrawal (blocks queued requests). External copy: Settings → Backup (`.llbackup`, share sheet) and Transfer (CSV, `.apkg`). Uninstalling removes app data; backups saved outside the app stay with the user. Backups are not encrypted (D-14) — the policy says so.

## 8. Platform requirements on submission day (step 7)

| Requirement | State 2026-11-17 | Check on submission day |
|---|---|---|
| Play target API | 36 (required for new apps/updates since 2026-08-31) | Play Console → Policy status; `REQUIRED_TARGET_SDK` in `release-check.mjs` |
| Play 16 KB page size | native libs built with 16 KB max page size (Phase 10) | Play Console app bundle explorer warning-free |
| Play AAB + Play App Signing | AAB built by the release workflow | enrol on first upload |
| Edge-to-edge (API 35+) | insets handled in `MainActivity` | device check (runbook checklist) |
| iOS SDK minimum for uploads | build with the Xcode version App Store Connect currently requires | Xcode release notes / App Store Connect warning |
| Privacy policy URL | in-app link from `VITE_PRIVACY_POLICY_URL`; tag requires it | URL live, same text as the form |

## Owner / legal checklist

- [ ] Publish the privacy policy ([draft](privacy-policy.md)) at an https URL; set repository variable `PRIVACY_POLICY_URL`.
- [ ] Legal review: controller role under BYOK, Data safety "shared" vs user-initiated, Apple "not collected", GDPR information duties (Art. 13), AI Act Art. 50, notice requirements of licences.
- [ ] Provider terms checked (no training on API data, retention, transfer basis) — date and links recorded in §1.
- [ ] Decide the AI report action (§4) — blocker for Play.
- [ ] IARC and Apple age-rating questionnaires answered from §5; results recorded.
- [ ] D-04 content rights or an explicit beta-only exception.
- [ ] D-08: stays "nothing sent" for the beta, or vendor chosen and the forms updated before the uploader ships.
- [ ] Trademark search for the app name.
