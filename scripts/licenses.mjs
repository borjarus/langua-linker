#!/usr/bin/env node
// Third-party licence report for the shipped mobile app (plan Phase 17 step 5, ADR-21).
//
// Collects every component that can end up in an Android or iOS build:
// - Rust: the normal dependency graph of the `langualinker` shell for the mobile targets
//   (`cargo metadata --filter-platform`), workspace crates excluded;
// - JavaScript: production packages of frontend/package-lock.json (over-inclusive: the Svelte
//   compiler's own dependencies are listed although only its runtime is bundled);
// - native: the Gradle dependencies of the Android app and the bundled SQLite library.
// Every licence expression must be satisfiable with the allowlist below; anything else fails.
//
// Usage:
//   node scripts/licenses.mjs            write src-tauri/third-party-licenses.json
//   node scripts/licenses.mjs --check    fail when the committed report is stale or a licence is not allowed
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
export const REPORT = join(ROOT, 'src-tauri', 'third-party-licenses.json')
const TARGETS = ['aarch64-linux-android', 'armv7-linux-androideabi', 'x86_64-linux-android', 'aarch64-apple-ios']
const ROOT_CRATE = 'langualinker'

/**
 * Licences accepted without further review. MPL-2.0 is file-level copyleft: allowed for
 * unmodified crates, the source is public (crates.io). Anything else needs a review and an entry here.
 */
export const ALLOWED = new Set([
  '0BSD',
  'Apache-2.0',
  'BSD-2-Clause',
  'BSD-3-Clause',
  'BSL-1.0',
  'CC0-1.0',
  'CDLA-Permissive-2.0',
  'ISC',
  'MIT',
  'MIT-0',
  'MPL-2.0',
  'Unicode-3.0',
  'Unicode-DFS-2016',
  'Unlicense',
  'Zlib',
  'LicenseRef-Public-Domain',
])

/** Native components Gradle/Xcode add next to the Rust and JS code (app/build.gradle.kts). */
const NATIVE = [
  { name: 'SQLite (bundled by libsqlite3-sys)', version: '3', license: 'LicenseRef-Public-Domain', url: 'https://sqlite.org/copyright.html' },
  { name: 'androidx.webkit:webkit', version: '1.6.1', license: 'Apache-2.0', url: 'https://developer.android.com/jetpack/androidx/releases/webkit' },
  { name: 'androidx.appcompat:appcompat', version: '1.6.1', license: 'Apache-2.0', url: 'https://developer.android.com/jetpack/androidx/releases/appcompat' },
  { name: 'com.google.android.material:material', version: '1.8.0', license: 'Apache-2.0', url: 'https://github.com/material-components/material-components-android' },
  { name: 'Kotlin standard library', version: '1.9', license: 'Apache-2.0', url: 'https://kotlinlang.org' },
]

/** `MIT/Apache-2.0` (old Cargo syntax) → `MIT OR Apache-2.0`. */
export function normalize(expression) {
  return expression
    .split('/')
    .map((part) => part.trim())
    .join(' OR ')
}

/** True when some choice of the SPDX expression uses only allowed licences. */
export function satisfiable(expression, allowed = ALLOWED) {
  const tokens = normalize(expression).replace(/[()]/g, ' $& ').split(/\s+/).filter(Boolean)
  let index = 0
  const peek = () => tokens[index]
  function factor() {
    const token = tokens[index++]
    if (token === undefined) throw new Error(`incomplete licence expression "${expression}"`)
    if (token === '(') {
      const value = or()
      if (tokens[index++] !== ')') throw new Error(`unbalanced licence expression "${expression}"`)
      return value
    }
    if (peek() === 'WITH') index += 2 // an exception only adds permissions
    return allowed.has(token)
  }
  function and() {
    let value = factor()
    while (peek() === 'AND') {
      index++
      value = factor() && value
    }
    return value
  }
  function or() {
    let value = and()
    while (peek() === 'OR') {
      index++
      value = and() || value
    }
    return value
  }
  const result = or()
  if (index !== tokens.length) throw new Error(`unexpected "${tokens[index]}" in licence expression "${expression}"`)
  return result
}

function cargoPackages() {
  const found = new Map()
  for (const target of TARGETS) {
    const metadata = JSON.parse(
      execFileSync('cargo', ['metadata', '--format-version', '1', '--locked', '--filter-platform', target], {
        cwd: ROOT,
        encoding: 'utf8',
        maxBuffer: 256 * 1024 * 1024,
      }),
    )
    const packages = new Map(metadata.packages.map((item) => [item.id, item]))
    const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]))
    const workspace = new Set(metadata.workspace_members)
    const root = metadata.packages.find((item) => item.name === ROOT_CRATE && workspace.has(item.id))
    const queue = [root.id]
    const seen = new Set(queue)
    while (queue.length > 0) {
      const node = nodes.get(queue.shift())
      for (const dep of node?.deps ?? []) {
        // Normal dependencies only: build scripts and dev-dependencies do not ship.
        if (!dep.dep_kinds.some((kind) => kind.kind === null) || seen.has(dep.pkg)) continue
        seen.add(dep.pkg)
        queue.push(dep.pkg)
      }
    }
    for (const id of seen) {
      if (workspace.has(id)) continue
      const item = packages.get(id)
      found.set(`${item.name}@${item.version}`, {
        name: item.name,
        version: item.version,
        license: item.license ? normalize(item.license) : '',
        url: item.repository ?? item.homepage ?? `https://crates.io/crates/${item.name}`,
        source: 'cargo',
      })
    }
  }
  return [...found.values()]
}

function npmPackages() {
  const lock = JSON.parse(readFileSync(join(ROOT, 'frontend', 'package-lock.json'), 'utf8'))
  return Object.entries(lock.packages)
    .filter(([path, item]) => path !== '' && !item.dev && !item.devOptional)
    .map(([path, item]) => {
      const name = path.slice(path.lastIndexOf('node_modules/') + 'node_modules/'.length)
      return { name, version: item.version, license: item.license ?? '', url: `https://www.npmjs.com/package/${name}`, source: 'npm' }
    })
}

export function buildReport() {
  const entries = [...cargoPackages(), ...npmPackages(), ...NATIVE.map((item) => ({ ...item, source: 'native' }))]
  const order = { native: 0, cargo: 1, npm: 2 }
  entries.sort((a, b) => order[a.source] - order[b.source] || a.name.localeCompare(b.name) || a.version.localeCompare(b.version))
  const problems = entries
    .filter((item) => !item.license || !satisfiable(item.license))
    .map((item) => `${item.source} ${item.name} ${item.version}: licence "${item.license || 'missing'}" is not on the allowlist`)
  return { entries, problems }
}

export function serialize(entries) {
  return `${JSON.stringify(entries, null, 1)}\n`
}

function main() {
  const check = process.argv.includes('--check')
  const { entries, problems } = buildReport()
  if (problems.length > 0) {
    console.error(problems.join('\n'))
    process.exit(1)
  }
  const text = serialize(entries)
  if (check) {
    let committed = ''
    try {
      committed = readFileSync(REPORT, 'utf8').replace(/\r\n/g, '\n')
    } catch {
      // missing report
    }
    if (committed !== text) {
      console.error(`${REPORT} is stale: run \`node scripts/licenses.mjs\` and commit the result.`)
      process.exit(1)
    }
    console.log(`Licence report up to date (${entries.length} components, all on the allowlist).`)
    return
  }
  writeFileSync(REPORT, text)
  console.log(`Wrote ${entries.length} components to ${REPORT}.`)
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main()
