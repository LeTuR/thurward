#!/bin/bash
# render-cloud-init.sh — render cloud-init user-data/meta-data and
# inject them into the *instance qcow2* under
# /var/lib/cloud/seed/nocloud/.  This is cloud-init's well-known
# NoCloud seed path; ds-identify always picks it up regardless of
# CD-ROM/blkid timing or virtio-vs-SATA quirks.
#
# We deliberately bypass the cloud-localds + seed-ISO route: the ISO
# approach turned out to be fragile on Q35 + virt-customised images
# (ds-identify wouldn't run, then with `ds=nocloud` cmdline cloud-init
# would read user-data from the (empty) cmdline instead of the ISO).
#
# Usage: render-cloud-init.sh <candidate> <role> <instance-qcow2> <pubkey-file>
#   candidate     = subdir under bench/candidates/ (e.g. nftables, trafficgen)
#   role          = sut | gen   (sut: full rules + nftables; gen: simpler)
#   instance-qcow2 = path to the per-instance qcow2 to inject into
#   pubkey-file   = path to SSH public key

set -euo pipefail

CAND="${1:?candidate (subdir under bench/candidates/) required}"
ROLE="${2:?role (sut|gen) required}"
INST_DISK="${3:?instance qcow2 path required}"
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

cp "$TEMPLATE_DIR/user-data" "$WORKDIR/user-data"
cp "$TEMPLATE_DIR/meta-data" "$WORKDIR/meta-data"

# Render @@HARNESS_SSH_KEY@@.
python3 - "$WORKDIR/user-data" "$PUBKEY" <<'PY'
import sys, pathlib
path = pathlib.Path(sys.argv[1])
key  = sys.argv[2]
path.write_text(path.read_text().replace("@@HARNESS_SSH_KEY@@", key))
PY

# For the nftables SUT, splice in the rules.nft contents.
if [ "$ROLE" = "sut" ] && [ -f "$BENCH_DIR/candidates/$CAND/rules.nft" ]; then
  RULES_PATH="$BENCH_DIR/candidates/$CAND/rules.nft"
  python3 - "$WORKDIR/user-data" "$RULES_PATH" <<'PY'
import sys, pathlib, textwrap
target = pathlib.Path(sys.argv[1])
rules  = pathlib.Path(sys.argv[2]).read_text()
indented = textwrap.indent(rules, "      ")
target.write_text(target.read_text().replace("      @@RULES_NFT@@", indented))
PY
fi

if ! command -v virt-customize >/dev/null 2>&1; then
  echo "ERROR: virt-customize not found (install guestfs-tools)" >&2
  exit 1
fi

# Build the virt-customize argument list.  Seed always goes in; the
# trex candidate also gets the pre-downloaded TRex tarball injected
# into /var/tmp/ so cloud-init can extract it offline (the bench
# bridges have no NAT).
declare -a VC_ARGS=(
  --mkdir /var/lib/cloud/seed/nocloud
  --upload "$WORKDIR/user-data:/var/lib/cloud/seed/nocloud/user-data"
  --upload "$WORKDIR/meta-data:/var/lib/cloud/seed/nocloud/meta-data"
)

if [ "$CAND" = "trex" ]; then
  TREX_TARBALL="$BENCH_DIR/images/trex/trex-latest.tar.gz"
  if [ ! -f "$TREX_TARBALL" ]; then
    echo "ERROR: TRex tarball missing at $TREX_TARBALL — run 'make trex-fetch' first" >&2
    exit 1
  fi
  VC_ARGS+=( --upload "$TREX_TARBALL:/var/tmp/trex-latest.tar.gz" )
fi

sudo virt-customize -a "$INST_DISK" "${VC_ARGS[@]}" >/dev/null

echo "  cloud-init seed injected into $INST_DISK${CAND:+ (candidate=$CAND)}"
