import assert from 'node:assert/strict'
import { test } from 'node:test'
import { normalize, satisfiable } from './licenses.mjs'

test('old Cargo slash syntax means OR', () => {
  assert.equal(normalize('MIT/Apache-2.0'), 'MIT OR Apache-2.0')
  assert.equal(normalize('Apache-2.0 / MIT'), 'Apache-2.0 OR MIT')
})

test('an expression passes when one choice uses only allowed licences', () => {
  assert.ok(satisfiable('MIT'))
  assert.ok(satisfiable('GPL-3.0-only OR MIT'))
  assert.ok(satisfiable('(MIT OR Apache-2.0) AND Unicode-3.0'))
  assert.ok(satisfiable('Apache-2.0 WITH LLVM-exception OR MIT'))
  assert.ok(satisfiable('Apache-2.0 AND ISC'))
})

test('copyleft, proprietary and unknown licences fail', () => {
  assert.ok(!satisfiable('GPL-3.0-only'))
  assert.ok(!satisfiable('MIT AND GPL-2.0-only'))
  assert.ok(!satisfiable('UNLICENSED'))
  assert.ok(!satisfiable('LGPL-2.1-or-later OR AGPL-3.0-only'))
})

test('malformed expressions are reported, not accepted', () => {
  assert.throws(() => satisfiable('(MIT OR Apache-2.0'))
  assert.throws(() => satisfiable('MIT OR'))
})
