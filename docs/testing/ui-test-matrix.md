# UI test matrix and diff acceptance rules

## Layers

| Layer | Tool | Runs | Proves |
|---|---|---|---|
| Contract & logic | Vitest (node + jsdom) | every PR | DTO rules, catalogue completeness, mock scenarios, router, query cache, i18n parity/plurals, study controller, chart scales/buckets (`tests/charts.test.ts`), job tracker and settings save queue |
| Components & screens | Vitest + `@testing-library/svelte` (jsdom) | every PR | roles/names, keyboard (Tab trap, Escape), disabled states, every route renders with mock data |
| Static | svelte-check (`--fail-on-warnings`), ESLint + eslint-plugin-svelte, `lint:i18n` | every PR | types, Svelte a11y warnings, no Tauri imports in UI, no literal UI strings |
| Release guard | `verify:release` | every PR | no demo adapter, fixtures or gallery in release bundles |
| Browser E2E & visual | Playwright + axe-core (`npm run test:e2e`, CI job `e2e`) | every PR since Phase 2 | real layout (rule 4) for the gallery matrix and every route at 200%/360 px, dialog/sheet/toast/Back behaviour, axe; Phase 3 learning flow ([e2e/learning.spec.ts](../../frontend/e2e/learning.spec.ts): onboarding → 20 ratings → summary → dashboard, keyboard, undo, card actions, leave guard, dashboard/study/summary at 200% in PL/EN with axe); Phase 4 management ([e2e/management.spec.ts](../../frontend/e2e/management.spec.ts): editor → details → edit → bulk tag, draft guard on Back, deck type/preset, details/editor/browser/deck and the preset dialog at 200% in PL/EN with axe; `layoutProblems(page, '[role="dialog"]')` checks only an open modal); Phase 5 ([e2e/phase5.spec.ts](../../frontend/e2e/phase5.spec.ts): onboarding → study → editor → statistics → CSV preview → restore, declined AI/telemetry/notifications still allow study, and statistics/settings/AI/privacy (reset dialog)/about/licences/transfer (dry run)/backup (restore dialog)/onboarding at 200% in PL/EN × light/dark with axe); ADR-03 frame profiling ([e2e/virtualization.spec.ts](../../frontend/e2e/virtualization.spec.ts): 50 000-card browser at 200%, slow and fast fling, bounded DOM everywhere, frame budget gated in desktop Chromium, WebKit and 4× CPU-throttled Chromium recorded only); gallery pixel snapshots once baselines are committed |
| Native smoke | physical Android/iOS WebView | milestones | keyboard resize, safe areas, TalkBack/VoiceOver, 200% system text |

jsdom does not lay out pages: layout, contrast, overflow at 200% and real scrolling are **not** proven by Vitest and remain open until Playwright/devices run.

## Browser / WebView matrix

| Target | Engine | Viewports | Themes | Text scale | Languages |
|---|---|---|---|---|---|
| Browser demo (dev) | Chromium, WebKit (Playwright) | 360×800, 768×1024, 1280×800 | light, dark | 100%, 200% | PL, EN |
| Android 8+ (API 26+) | Android System WebView (Chromium) | phone portrait/landscape, tablet | system | system 100% / 200% | PL, EN |
| iOS 16+ | WKWebView | iPhone SE, iPhone 15, iPad | system | Dynamic Type default / AX5 | PL, EN |
| macOS (dev shell only) | WKWebView | 960×720 window | light, dark | 100% | PL |

## Gallery snapshots

`/gallery` renders every design-system component with demo data. Snapshot set = gallery × {PL, EN} × {light, dark} × {100%, 200%} at 360 px and 1280 px, in Chromium and WebKit ([e2e/gallery.spec.ts](../../frontend/e2e/gallery.spec.ts)). Every cell also runs the rule-4 layout check, the dialog/sheet/toast/Back flow and axe, without pixels.

Baselines live in `frontend/e2e/__snapshots__/` without a platform suffix. Generate them with the manual `E2E baselines` workflow, review the artifact and commit it; from then on the `e2e` CI job compares them (`PW_SNAPSHOTS=1`). Locally (Windows/macOS) snapshot tests are skipped.

Known engine quirk: Playwright's WebKit build reports stale computed colours for the demo banner in dark mode although the pixels are correct, so axe `color-contrast` is evaluated in Chromium only (the colour tokens are shared).

## Diff acceptance rules

1. Any pixel diff in a snapshot requires review; the PR description names the intended change.
2. Unintended diffs > 0.1% of pixels in a component region block the merge.
3. Font-rendering noise (anti-aliasing ≤ 1 px, < 0.05%) may be accepted once per engine update with a regenerated baseline in a separate commit.
4. At 200% text no control may be clipped, overlap another control, or require horizontal scrolling (except the virtual list's own content).
5. axe-core: zero violations of `serious`/`critical` impact; `moderate` require an issue link.
6. Baselines are regenerated only on Linux CI images to avoid OS font differences.
