#!/usr/bin/env node
// Release engineering gate (plan Phase 17 steps 7–9, ADR-21). Runs in CI on every push and before
// a tag is published.
//
//   node scripts/release-check.mjs            consistency checks (every push)
//   node scripts/release-check.mjs --tag vX   also: the tag matches, CHANGELOG has the entry,
//                                             a final (store) build has its privacy-policy URL;
//                                             a pre-release without it only warns
//
// Checks: one version in every manifest; the Android versionCode and the MSI version follow the
// scheme below; Android targets the API level Google Play requires on submission day; the app
// manifest declares only reviewed permissions and no TV/leanback launcher.
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')

/**
 * Google Play: new apps and updates must target Android 16 (API 36) since 2026-08-31.
 * Raise this when Play announces the next level (expected August 2027, API 37).
 */
export const REQUIRED_TARGET_SDK = 36

/** Permissions our own manifest may declare; plugin manifests (notification) are merged by Gradle. */
export const ALLOWED_PERMISSIONS = new Set(['android.permission.INTERNET'])

const SEMVER = /^(\d+)\.(\d+)\.(\d+)(?:-(alpha|beta|rc)\.(\d+))?$/

/** Parses the versions this project uses: `X.Y.Z` and `X.Y.Z-(alpha|beta|rc).N`. */
export function parseVersion(version) {
  const match = SEMVER.exec(version)
  if (!match) throw new Error(`version "${version}" is not X.Y.Z or X.Y.Z-(alpha|beta|rc).N`)
  const [, major, minor, patch, channel, number] = match
  return { major: +major, minor: +minor, patch: +patch, channel: channel ?? null, number: number === undefined ? null : +number }
}

/**
 * Android versionCode: MMmmppss — major·10⁶ + minor·10⁴ + patch·100 + suffix, where the suffix is
 * alpha.N → N, beta.N → 30+N, rc.N → 60+N (N = 1–29) and 99 for the final release. Strictly
 * increasing along semver order, so every upload to Play gets a higher code (Tauri's default,
 * major·10⁶ + minor·10³ + patch, ignores the pre-release and repeats the code of 0.4.0-beta.1 in
 * 0.4.0-beta.2).
 */
export function androidVersionCode(version) {
  const { major, minor, patch, channel, number } = parseVersion(version)
  if (minor > 99 || patch > 99) throw new Error(`version "${version}": minor and patch must be ≤ 99`)
  let suffix = 99
  if (channel !== null) {
    const base = { alpha: 0, beta: 30, rc: 60 }[channel]
    if (number < 1 || number > 29) throw new Error(`version "${version}": pre-release number must be 1–29`)
    suffix = base + number
  }
  return major * 1_000_000 + minor * 10_000 + patch * 100 + suffix
}

/** MSI accepts only numeric versions: a pre-release keeps X.Y.Z.N, a final release needs none. */
export function wixVersion(version) {
  const { major, minor, patch, number } = parseVersion(version)
  return number === null ? null : `${major}.${minor}.${patch}.${number}`
}

export function manifestProblems(xml) {
  const problems = []
  for (const [, permission] of xml.matchAll(/<uses-permission[^>]*android:name="([^"]+)"/g)) {
    if (!ALLOWED_PERMISSIONS.has(permission)) problems.push(`AndroidManifest.xml declares ${permission}, which is not reviewed (ALLOWED_PERMISSIONS, Data Safety)`)
  }
  if (xml.includes('LEANBACK_LAUNCHER') || xml.includes('android.software.leanback')) {
    problems.push('AndroidManifest.xml declares Android TV support; the app is not reviewed for TV')
  }
  return problems
}

export function gradleProblems(gradle) {
  const problems = []
  const target = +(/targetSdk\s*=\s*(\d+)/.exec(gradle)?.[1] ?? 0)
  const compile = +(/compileSdk\s*=\s*(\d+)/.exec(gradle)?.[1] ?? 0)
  if (target < REQUIRED_TARGET_SDK) problems.push(`targetSdk ${target} is below the Play requirement ${REQUIRED_TARGET_SDK}`)
  if (compile < target) problems.push(`compileSdk ${compile} is below targetSdk ${target}`)
  return problems
}

export function changelogHas(changelog, version) {
  return new RegExp(`^## \\[${version.replace(/\./g, '\\.')}\\]`, 'm').test(changelog)
}

/**
 * The privacy-policy URL of a tagged build: a final release goes to the stores and needs it; a
 * pre-release (`-alpha/-beta/-rc.N`) published on GitHub only may ship without the link (owner
 * decision 2026-11-17, ADR-21). Returns `{ problem }`, `{ warning }` or `{}`.
 */
export function policyUrlCheck(version, env) {
  if (env.VITE_PRIVACY_POLICY_URL?.startsWith('https://')) return {}
  if (parseVersion(version).channel !== null) {
    return { warning: `VITE_PRIVACY_POLICY_URL is not set: pre-release ${version} ships without the privacy-policy link (GitHub only, not for a store upload)` }
  }
  return { problem: 'VITE_PRIVACY_POLICY_URL (https) must be set for a store build: the app links to the published policy' }
}

function read(path) {
  return readFileSync(join(ROOT, path), 'utf8')
}

export function check({ tag = null, env = process.env, warnings = [] } = {}) {
  const problems = []
  const tauri = JSON.parse(read('src-tauri/tauri.conf.json'))
  const version = tauri.version
  try {
    parseVersion(version)
  } catch (error) {
    return [error.message]
  }

  const cargo = /^version\s*=\s*"([^"]+)"/m.exec(read('src-tauri/Cargo.toml'))?.[1]
  const npm = JSON.parse(read('frontend/package.json')).version
  const lock = /name = "langualinker"\nversion = "([^"]+)"/.exec(read('Cargo.lock').replace(/\r\n/g, '\n'))?.[1]
  for (const [file, value] of [
    ['src-tauri/Cargo.toml', cargo],
    ['Cargo.lock', lock],
    ['frontend/package.json', npm],
  ]) {
    if (value !== version) problems.push(`${file} has version ${value}, tauri.conf.json has ${version}`)
  }

  const code = tauri.bundle?.android?.versionCode
  if (code !== androidVersionCode(version)) {
    problems.push(`bundle.android.versionCode is ${code}, expected ${androidVersionCode(version)} for ${version}`)
  }
  const wix = tauri.bundle?.windows?.wix?.version ?? null
  if (wix !== wixVersion(version)) problems.push(`bundle.windows.wix.version is ${wix}, expected ${wixVersion(version)} for ${version}`)

  problems.push(...gradleProblems(read('src-tauri/gen/android/app/build.gradle.kts')))
  problems.push(...manifestProblems(read('src-tauri/gen/android/app/src/main/AndroidManifest.xml')))

  if (tag !== null) {
    if (tag !== `v${version}`) problems.push(`tag ${tag} does not match version ${version}`)
    if (!changelogHas(read('CHANGELOG.md'), version)) problems.push(`CHANGELOG.md has no "## [${version}]" entry`)
    const policy = policyUrlCheck(version, env)
    if (policy.problem) problems.push(policy.problem)
    if (policy.warning) warnings.push(policy.warning)
  }
  return problems
}

function main() {
  const at = process.argv.indexOf('--tag')
  const tag = at === -1 ? null : process.argv[at + 1]
  const warnings = []
  const problems = check({ tag, warnings })
  for (const line of warnings) console.log(`::warning::release-check: ${line}`)
  if (problems.length > 0) {
    console.error(problems.map((line) => `release-check: ${line}`).join('\n'))
    process.exit(1)
  }
  console.log('release-check: versions, Android target SDK and manifest are consistent.')
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main()
