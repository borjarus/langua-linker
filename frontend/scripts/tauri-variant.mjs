#!/usr/bin/env node
// Runs the Tauri CLI for one app variant, so dev/beta/release builds get their own identifier,
// name and storage and can be installed side by side.
// Usage: node scripts/tauri-variant.mjs <dev|beta|release> <tauri args...>
//   node scripts/tauri-variant.mjs dev dev
//   node scripts/tauri-variant.mjs beta android build --apk
//
// - tauri.conf.json is the release variant; tauri.<variant>.conf.json overlays name, identifier
//   and the frontend build for dev/beta.
// - Android/iOS projects are generated for the base identifier (the CLI derives the Kotlin package
//   from it), so mobile builds re-apply it via tauri.mobile.conf.json; Gradle adds the
//   `.dev`/`.beta` application id suffix from LL_APP_VARIANT instead.
// - LL_APP_VARIANT is also compiled into the Rust shell (`AppInfoDto.mode`).
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const VARIANTS = ['dev', 'beta', 'release']
const MOBILE = ['android', 'ios']

export function tauriArgs(variant, args) {
  if (!VARIANTS.includes(variant)) throw new Error(`Unknown variant "${variant}" (expected ${VARIANTS.join('|')})`)
  if (args.length === 0) throw new Error('Missing Tauri command, e.g. "dev" or "android build --apk"')
  const mobile = MOBILE.includes(args[0])
  const configs = []
  if (variant !== 'release') configs.push(`../src-tauri/tauri.${variant}.conf.json`)
  if (mobile) configs.push('../src-tauri/tauri.mobile.conf.json')
  // Options go right after the (sub)command so anything after a `--` still reaches the app.
  const commandLength = mobile ? 2 : 1
  return [...args.slice(0, commandLength), ...configs.flatMap((config) => ['--config', config]), ...args.slice(commandLength)]
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [variant, ...rest] = process.argv.slice(2)
  let args
  try {
    args = tauriArgs(variant, rest)
  } catch (error) {
    console.error(`${error.message}\nUsage: node scripts/tauri-variant.mjs <${VARIANTS.join('|')}> <tauri args...>`)
    process.exit(2)
  }
  const windows = process.platform === 'win32'
  const result = spawnSync(windows ? 'tauri.cmd' : 'tauri', args, {
    cwd: fileURLToPath(new URL('..', import.meta.url)),
    stdio: 'inherit',
    // Node refuses to spawn .cmd files without a shell; the arguments contain no spaces or quotes.
    shell: windows,
    env: {
      ...process.env,
      PATH: `${fileURLToPath(new URL('../node_modules/.bin', import.meta.url))}${windows ? ';' : ':'}${process.env.PATH}`,
      TAURI_APP_PATH: '../src-tauri',
      LL_APP_VARIANT: variant,
    },
  })
  process.exit(result.status ?? 1)
}
