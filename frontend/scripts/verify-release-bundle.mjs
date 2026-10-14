#!/usr/bin/env node
// Fails when a release bundle contains the demo adapter or the debug gallery.
// Usage: node scripts/verify-release-bundle.mjs <distDir>
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'

const dist = process.argv[2] ?? 'dist-release'
const FORBIDDEN = [
  ['LL_DEMO_ADAPTER_INCLUDED', 'demo adapter (MockAppApi)'],
  ['demo-mnemonic-v1', 'demo fixtures'],
  ['GalleryScreen', 'debug gallery'],
  ['DiagnosticsScreen', 'native diagnostics screen'],
  ['probe_secret_store', 'native diagnostics commands'],
  ['probe_file_open', 'native diagnostics commands'],
  ['plugin:notification|', 'notification spike'],
]

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name)
    return statSync(path).isDirectory() ? walk(path) : [path]
  })
}

const files = walk(dist)
const problems = []
for (const file of files) {
  const content = readFileSync(file, 'utf8')
  for (const [needle, label] of FORBIDDEN) {
    if (file.includes(needle) || content.includes(needle)) problems.push(`${file}: contains ${label}`)
  }
}

if (files.length === 0) problems.push(`${dist}: no files — build the release profile first`)
if (problems.length > 0) {
  console.error(problems.join('\n'))
  process.exit(1)
}
console.log(`Release bundle ${dist} is free of demo code (${files.length} files checked).`)
