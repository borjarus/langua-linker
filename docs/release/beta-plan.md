# Closed beta plan (Phase 17 step 3)

Status: ready to start once the [release checklist](release-checklist.md) "beta" column is green. Results go in the table at the end; a small cohort does not prove population D7 or retention — report numbers with their sample size.

## Setup

| Item | Plan |
|---|---|
| Duration | at least 2 weeks of daily use after the last tester joined; extend if a critical bug resets the clock |
| Cohort | 20–50 Polish-speaking adult learners of German or English, A1–B1, mixed Android/iPhone; at least 5 per platform |
| Channels | Google Play **closed testing** track (testers' Google accounts or a group) — note Play's rule for new personal developer accounts: a closed test with ≥ 12 testers for ≥ 14 days before production access; Apple **TestFlight** external testing (needs Beta App Review) |
| Build | release variant from a `vX.Y.Z-beta.N` tag, store-signed, same privacy policy and forms as production |
| Content | candidate decks only with the owner's recorded D-04 beta exception |
| AI | BYOK: testers use their own key; a tester without a key tests the "AI off" path. No shared developer key (D-02/D-03) |
| Metrics | opt-in local metrics (ADR-13); nothing is uploaded (D-08). Testers who agree paste Settings → Privacy → "Copy summary" into the weekly form |

## Onboarding message to testers (PL, short)

> Dziękujemy za udział w becie LanguaLinker. Ucz się codziennie przez co najmniej 2 tygodnie. Raz w tygodniu wypełnij krótki formularz [link]; jeśli zgodzisz się na statystyki (Ustawienia → Prywatność), wklej tam „Kopiuj podsumowanie”. Błędy zgłaszaj przez [kanał], dołączając „Zdarzenia diagnostyczne” z ekranu O aplikacji (bez treści kart). Przed aktualizacją zrób kopię zapasową (Ustawienia → Kopia zapasowa → Udostępnij).

## Daily triage (step 14)

Every working day, 15 minutes: new reports, Play Console vitals (crashes/ANRs, available without our SDK), App Store Connect crashes (TestFlight).

| Severity | Definition | Response |
|---|---|---|
| S1 critical | lost or corrupted ratings/notes, data visible to others, key leak, crash loop at start | stop distributing the build (runbook §2), hotfix |
| S2 major | a primary flow blocked (study, add card, backup/restore, onboarding), crash in a frequent path | fix in the next beta build (≤ 3 days) |
| S3 minor | wrong text, layout, an association of poor quality, slow screen | batch into the next build |

Association quality reports (wrong, offensive, unclear mnemonic) are tagged `content` and go to the native review list in `content/PROVENANCE.md`.

## Weekly tester form

1. Days studied this week; minutes per day (estimate).
2. Did anything lose or change your progress? (yes → S1 path)
3. Best and worst association you saw (copy the text).
4. Did you use AI associations? Provider; anything wrong?
5. Reminders: helpful / annoying / off.
6. Optional: pasted metrics summary.

## Exit criteria (step 15)

- [ ] No open S1; no S2 older than one build.
- [ ] Data drill on testers' real data: at least 5 testers did backup → reinstall (or second device) → restore with the counts matching.
- [ ] Crash-free starts ≥ 99.5 % in Play vitals / App Store Connect *and* in the opted-in summaries — reported with the number of starts and the share of testers who opted in.
- [ ] Cold start median < 2 s and generation p90 < 4 s on the testers' devices that reported metrics (sample size stated).
- [ ] D7: share of testers active on day 7, with cohort size — a signal only, not the population target.
- [ ] Owner signs off the results below.

## Results

| Build | Dates | Testers (Android/iOS) | Opted-in metrics | S1/S2/S3 found/fixed | Crash-free starts (n) | D7 (n) | Notes |
|---|---|---|---|---|---|---|---|
| | | | | | | | |
