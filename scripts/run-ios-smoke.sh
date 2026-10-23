#!/usr/bin/env bash
set -euo pipefail

APP_ID="com.mila.langualinker"
BUNDLE="src-tauri/gen/apple/build/arm64-sim/LanguaLinker Dev.app"
OUT_DIR="docs/release/screenshots"

mkdir -p "$OUT_DIR"

echo "Terminating and reinstalling..."
xcrun simctl terminate booted "$APP_ID" || true
xcrun simctl install booted "$BUNDLE"

echo "Launching..."
xcrun simctl launch booted "$APP_ID"

STEPS=(
  "00-home"
  "01-browser"
  "02-card-details"
  "03-editor"
  "04-deck-overview"
  "05-stats"
  "06-settings"
  "07-settings-ai"
  "08-settings-privacy"
  "09-settings-about"
  "10-settings-licences"
  "11-settings-transfer"
  "12-settings-backup"
  "13-onboarding"
)

echo "Waiting for tour to start..."
sleep 2.0

for step in "${STEPS[@]}"; do
  sleep 2.0
  echo "Capturing $step..."
  xcrun simctl io booted screenshot "$OUT_DIR/ios-phase-$step.png"
  sleep 2.0
done

echo "Tour capture completed!"
