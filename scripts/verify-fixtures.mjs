#!/usr/bin/env node
// Verifies migrated reference assets (bundled content, KMP schema, APKG, FSRS spec) against
// scripts/fixtures.lock.json: SHA-256, size and NDJSON record counts.
//   node scripts/verify-fixtures.mjs            check (CI)
//   node scripts/verify-fixtures.mjs --update   rewrite the lock after a reviewed change
import { createHash } from 'node:crypto'
import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = fileURLToPath(new URL('..', import.meta.url))
const LOCK = join(ROOT, 'scripts', 'fixtures.lock.json')
const TRACKED_DIRS = ['content/decks', 'tests/fixtures/kmp', 'tests/fixtures/fsrs']

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name)
    return statSync(path).isDirectory() ? walk(path) : [path]
  })
}

function describe(path) {
  const bytes = readFileSync(path)
  const entry = { sha256: createHash('sha256').update(bytes).digest('hex'), bytes: bytes.length }
  if (path.endsWith('.ndjson')) {
    entry.records = bytes.toString('utf8').split('\n').filter((line) => line.trim() !== '').length
  }
  return entry
}

const current = Object.fromEntries(
  TRACKED_DIRS.flatMap((dir) => walk(join(ROOT, dir)))
    .map((path) => [relative(ROOT, path).replaceAll('\\', '/'), describe(path)])
    .sort(([a], [b]) => a.localeCompare(b)),
)

if (process.argv.includes('--update')) {
  const previous = JSON.parse(readFileSync(LOCK, 'utf8'))
  writeFileSync(LOCK, `${JSON.stringify({ ...previous, files: current }, null, 2)}\n`)
  console.log(`Updated ${relative(ROOT, LOCK)} (${Object.keys(current).length} files).`)
  process.exit(0)
}

const lock = JSON.parse(readFileSync(LOCK, 'utf8'))
const problems = []
for (const [path, expected] of Object.entries(lock.files)) {
  const actual = current[path]
  if (!actual) problems.push(`${path}: missing`)
  else if (actual.sha256 !== expected.sha256) problems.push(`${path}: checksum differs from the reviewed lock`)
}
for (const path of Object.keys(current)) {
  if (!lock.files[path]) problems.push(`${path}: not in fixtures.lock.json (review, then run with --update)`)
}

const records = Object.entries(current)
  .filter(([path]) => path.startsWith('content/decks/') && path.endsWith('.ndjson'))
  .reduce((sum, [, entry]) => sum + (entry.records ?? 0), 0)
if (records !== lock.expectedContentRecords) {
  problems.push(`content/decks: ${records} NDJSON records, expected ${lock.expectedContentRecords}`)
}

if (problems.length > 0) {
  console.error(problems.join('\n'))
  process.exit(1)
}
console.log(`Fixtures verified: ${Object.keys(current).length} files, ${records} bundled content records.`)
