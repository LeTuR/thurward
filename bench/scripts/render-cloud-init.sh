#!/bin/bash
# render-cloud-init.sh — substitute the harness SSH key (and, for the
# nftables candidate, the rules.nft contents) into the cloud-init
# templates, then pack a seed ISO that libvirt can attach as a virtio
# CD-ROM.
#
# Usage: render-cloud-init.sh <candidate> <role> <out-iso> <pubkey-file>
#   candidate = subdir name under bench/candidates/ (e.g. nftables, trafficgen)
#   role      = sut | gen   (sut: full rules + nftables; gen: simpler)
#   out-iso   = path to write seed ISO
#   pubkey-file = path to SSH public key

set -euo pipefail

CAND="${1:?candidate (subdir under bench/candidates/) required}"
ROLE="${2:?role (sut|gen) required}"
OUT_ISO="${3:?output ISO path required}"
PUBKEY_FILE="${4:?SSH public key path required}"

if [ ! -f "$PUBKEY_FILE" ]; then
  echo "ERROR: pubkey not found: $PUBKEY_FILE" >&2
  exit 1
fi

BENCH_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TEMPLATE_DIR="$BENCH_DIR/candidates/$CAND/cloud-init"

if [ ! -d "$TEMPLATE_DIR" ]; then
  echo "ERROR: no cloud-init dir at $TEMPLATE_DIR" >&2
  exit 1
fi

PUBKEY="$(cat "$PUBKEY_FILE")"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

# Copy templates into the workdir so we can substitute in place.
cp "$TEMPLATE_DIR/user-data" "$WORKDIR/user-data"
cp "$TEMPLATE_DIR/meta-data" "$WORKDIR/meta-data"

# Render @@HARNESS_SSH_KEY@@.
# Use Python instead of sed because the pubkey contains slashes and
# `+` characters that mis-trigger sed.
python3 - "$WORKDIR/user-data" "$PUBKEY" <<'PY'
import sys, pathlib
path = pathlib.Path(sys.argv[1])
key  = sys.argv[2]
text = path.read_text()
text = text.replace("@@HARNESS_SSH_KEY@@", key)
path.write_text(text)
PY

# For the nftables SUT, splice in the rules.nft contents.
if [ "$ROLE" = "sut" ] && [ -f "$BENCH_DIR/candidates/$CAND/rules.nft" ]; then
  RULES_PATH="$BENCH_DIR/candidates/$CAND/rules.nft"
  python3 - "$WORKDIR/user-data" "$RULES_PATH" <<'PY'
import sys, pathlib, textwrap
target = pathlib.Path(sys.argv[1])
rules  = pathlib.Path(sys.argv[2]).read_text()
# cloud-init's content block under write_files needs each line indented
# by 6 spaces (4 for the YAML "    content: |" + 2 for the literal block).
indented = textwrap.indent(rules, "      ")
text = target.read_text().replace("      @@RULES_NFT@@", indented)
target.write_text(text)
PY
fi

# Build the seed ISO.
if ! command -v cloud-localds >/dev/null 2>&1; then
  echo "ERROR: cloud-localds not found (install cloud-image-utils)" >&2
  exit 1
fi

cloud-localds "$OUT_ISO" "$WORKDIR/user-data" "$WORKDIR/meta-data"
echo "  seed ISO: $OUT_ISO"
