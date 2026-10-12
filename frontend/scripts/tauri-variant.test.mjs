// node --test scripts/tauri-variant.test.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { tauriArgs } from './tauri-variant.mjs'

const DEV = ['--config', '../src-tauri/tauri.dev.conf.json']
const MOBILE = ['--config', '../src-tauri/tauri.mobile.conf.json']

test('release uses the base config only', () => {
  assert.deepEqual(tauriArgs('release', ['build']), ['build'])
})

test('desktop variants overlay their own config after the command', () => {
  assert.deepEqual(tauriArgs('dev', ['dev', '--', '--app-arg']), ['dev', ...DEV, '--', '--app-arg'])
})

test('mobile builds restore the base identifier after the variant overlay', () => {
  assert.deepEqual(tauriArgs('dev', ['android', 'build', '--apk']), ['android', 'build', ...DEV, ...MOBILE, '--apk'])
  assert.deepEqual(tauriArgs('release', ['ios', 'dev']), ['ios', 'dev', ...MOBILE])
})

test('unknown variants and empty commands are rejected', () => {
  assert.throws(() => tauriArgs('prod', ['build']), /Unknown variant/)
  assert.throws(() => tauriArgs('dev', []), /Missing Tauri command/)
})
