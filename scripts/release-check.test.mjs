// node --test scripts/release-check.test.mjs scripts/licenses.test.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { androidVersionCode, changelogHas, check, gradleProblems, manifestProblems, parseVersion, policyUrlCheck, wixVersion } from './release-check.mjs'

test('versionCode grows along semver order, pre-releases included', () => {
  const order = ['0.3.0', '0.4.0-alpha.1', '0.4.0-beta.1', '0.4.0-beta.2', '0.4.0-rc.1', '0.4.0', '0.4.1-beta.1', '0.4.1', '0.5.0', '1.0.0']
  const codes = order.map(androidVersionCode)
  for (let i = 1; i < codes.length; i++) assert.ok(codes[i] > codes[i - 1], `${order[i]} (${codes[i]}) > ${order[i - 1]} (${codes[i - 1]})`)
  assert.equal(androidVersionCode('0.4.0-beta.1'), 40031)
  assert.equal(androidVersionCode('1.2.3'), 1_020_399)
  // Tauri's default code of the already published 0.4.0-beta.1 was 4000; the new scheme continues above it.
  assert.ok(androidVersionCode('0.4.0-beta.2') > 4000)
})

test('versions outside the scheme are refused', () => {
  assert.throws(() => parseVersion('1.0'))
  assert.throws(() => parseVersion('1.0.0-preview.1'))
  assert.throws(() => androidVersionCode('1.100.0'))
  assert.throws(() => androidVersionCode('1.0.0-beta.30'))
  assert.throws(() => androidVersionCode('1.0.0-beta.0'))
})

test('MSI version carries the pre-release number only', () => {
  assert.equal(wixVersion('0.4.0-beta.1'), '0.4.0.1')
  assert.equal(wixVersion('1.0.0'), null)
})

test('manifest: only reviewed permissions, no TV launcher', () => {
  assert.deepEqual(manifestProblems('<uses-permission android:name="android.permission.INTERNET" />'), [])
  assert.equal(manifestProblems('<uses-permission android:name="android.permission.READ_CONTACTS" />').length, 1)
  assert.equal(manifestProblems('<category android:name="android.intent.category.LEANBACK_LAUNCHER" />').length, 1)
})

test('gradle: target SDK meets the Play requirement', () => {
  assert.deepEqual(gradleProblems('compileSdk = 36\n targetSdk = 36'), [])
  assert.equal(gradleProblems('compileSdk = 34\n targetSdk = 34').length, 1)
  assert.equal(gradleProblems('compileSdk = 35\n targetSdk = 36').length, 1)
})

test('changelog entry lookup is exact', () => {
  const log = '# Changelog\n\n## [0.4.0-beta.1] — 2026-11-16\n'
  assert.ok(changelogHas(log, '0.4.0-beta.1'))
  assert.ok(!changelogHas(log, '0.4.0'))
})

test('the repository passes its own gate', () => {
  assert.deepEqual(check(), [])
})

test('a tag needs the matching version', () => {
  const problems = check({ tag: 'v9.9.9', env: { VITE_PRIVACY_POLICY_URL: 'https://example.org/privacy' } })
  assert.ok(problems.some((line) => line.includes('does not match')))
})

test('a final release needs a privacy-policy URL; a pre-release only warns', () => {
  assert.ok(policyUrlCheck('1.0.0', {}).problem)
  assert.ok(policyUrlCheck('1.0.0', { VITE_PRIVACY_POLICY_URL: 'http://example.org' }).problem)
  assert.deepEqual(policyUrlCheck('1.0.0', { VITE_PRIVACY_POLICY_URL: 'https://example.org/privacy' }), {})
  assert.ok(policyUrlCheck('0.5.0-beta.1', {}).warning)
  assert.equal(policyUrlCheck('0.5.0-beta.1', {}).problem, undefined)
})
