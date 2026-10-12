# UI test matrix and diff acceptance rules

## Layers

| Layer | Tool | Runs | Proves |
|---|---|---|---|
| Contract & logic | Vitest (node + jsdom) | every PR | DTO rules, catalogue completeness, mock scenarios, router, query cache, i18n parity/plurals, study controller |
| Components & screens | Vitest + `@testing-library/svelte` (jsdom) | every PR | roles/names, keyboard (Tab trap, Escape), disabled states, every route renders with mock data |
| Static | svelte-check (`--fail-on-warnings`), ESLint + eslint-plugin-svelte, `lint:i18n` | every PR | types, Svelte a11y warnings, no Tauri imports in UI, no literal UI strings |
| Release guard | `verify:release` | every PR | no demo adapter, fixtures or gallery in release bundles |
| Browser E2E & visual | Playwright + axe-core | from Phase 3 | real layout, contrast, gallery snapshots |
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

`/gallery` renders every design-system component with demo data. Snapshot set = gallery × {PL, EN} × {light, dark} × {100%, 200%} at 360 px and 1280 px.

## Diff acceptance rules

1. Any pixel diff in a snapshot requires review; the PR description names the intended change.
2. Unintended diffs > 0.1% of pixels in a component region block the merge.
3. Font-rendering noise (anti-aliasing ≤ 1 px, < 0.05%) may be accepted once per engine update with a regenerated baseline in a separate commit.
4. At 200% text no control may be clipped, overlap another control, or require horizontal scrolling (except the virtual list's own content).
5. axe-core: zero violations of `serious`/`critical` impact; `moderate` require an issue link.
6. Baselines are regenerated only on Linux CI images to avoid OS font differences.
