#!/usr/bin/env bash
# Re-render every thurward architecture diagram from its spec.
# Idempotent. Run from anywhere.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$(cd "$HERE/.." && pwd)"
TMP="$(mktemp -d -t thurward-diagrams.XXXX)"
trap 'rm -rf "$TMP"' EXIT

for spec in "$HERE"/*.spec.json; do
  name="$(basename "$spec" .spec.json)"
  echo "--- $name"
  python3 "$HERE/build_diagram.py" "$spec" "$TMP/$name.xml"
  xmllint --noout "$TMP/$name.xml"
  xvfb-run -a drawio --no-sandbox -x -f svg -e \
    -o "$OUT/$name.svg" "$TMP/$name.xml" 2>/dev/null
  printf "    %s\n" "$OUT/$name.svg ($(stat -c%s "$OUT/$name.svg") bytes)"
done
