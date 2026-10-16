#!/bin/zsh
# Compile OshO Squad et remplace /Applications/OshO Squad.app.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

npm ci --no-audit --no-fund
npm run tauri build -- --bundles app --config src-tauri/tauri.osho.conf.json

app="${CARGO_TARGET_DIR:-src-tauri/target}/release/bundle/macos/OshO Squad.app"
if pgrep -f "/Applications/OshO Squad.app/" >/dev/null; then
  read -q "?OshO Squad tourne : le quitter (les agents en cours s'arrêtent) ? [y/N] " || { echo; exit 1; }
  echo
  osascript -e 'quit app "OshO Squad"'
  while pgrep -f "/Applications/OshO Squad.app/" >/dev/null; do sleep 1; done
fi
rm -rf "/Applications/OshO Squad.app"
cp -R "$app" /Applications/
open "/Applications/OshO Squad.app"
echo "OshO Squad installé : $(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist")"
