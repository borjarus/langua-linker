#!/usr/bin/env node
// Fails when a .svelte file contains user-visible literal text instead of an i18n key.
// Checks markup text nodes and static values of user-facing attributes. Text without letters
// (punctuation, separators, emoji) is allowed. Exceptions must be documented in ALLOWED below.
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { parse } from 'svelte/compiler'

const ROOT = new URL('../src/', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')
const USER_FACING_ATTRIBUTES = new Set(['aria-label', 'aria-description', 'title', 'placeholder', 'alt', 'label'])
const LETTER = /\p{L}/u
/** Documented technical exceptions: `file:text`. */
const ALLOWED = new Set([])

function walkFiles(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name)
    return statSync(path).isDirectory() ? walkFiles(path) : path.endsWith('.svelte') ? [path] : []
  })
}

function lineOf(source, offset) {
  return source.slice(0, offset).split('\n').length
}

function check(file) {
  const source = readFileSync(file, 'utf8')
  const ast = parse(source, { modern: true })
  const problems = []
  const rel = relative(ROOT, file).replaceAll('\\', '/')

  const report = (text, start) => {
    const trimmed = text.trim()
    if (!LETTER.test(trimmed) || ALLOWED.has(`${rel}:${trimmed}`)) return
    problems.push(`${rel}:${lineOf(source, start)}  literal UI text: ${JSON.stringify(trimmed)}`)
  }

  const visit = (node) => {
    if (!node || typeof node !== 'object') return
    if (Array.isArray(node)) return node.forEach(visit)
    if (node.type === 'Text') report(node.data, node.start)
    if ((node.type === 'RegularElement' || node.type === 'Component' || node.type === 'SvelteElement') && Array.isArray(node.attributes)) {
      for (const attribute of node.attributes) {
        if (attribute.type !== 'Attribute' || !USER_FACING_ATTRIBUTES.has(attribute.name) || !Array.isArray(attribute.value)) continue
        for (const part of attribute.value) if (part.type === 'Text') report(part.data, part.start)
      }
    }
    for (const [key, value] of Object.entries(node)) {
      if (key === 'attributes' || key === 'expression' || key === 'metadata') continue
      if (value && typeof value === 'object') visit(value)
    }
  }

  visit(ast.fragment)
  return problems
}

const problems = walkFiles(ROOT).flatMap(check)
if (problems.length > 0) {
  console.error(problems.join('\n'))
  console.error(`\n${problems.length} literal UI string(s) found. Use i18n keys (src/locales).`)
  process.exit(1)
}
console.log('No literal UI strings in .svelte files.')
