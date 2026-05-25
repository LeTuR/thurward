#!/bin/bash
# b00.sh — orchestrate B-00 (reference path with empty ruleset) on the
# nftables candidate using TRex + MLRsearch-flavoured binary NDR/PDR
# search. Per tests/benchmarks.md § 0.6, this is a per-environment
# control: the result tells us where the virtio path tops out.
#
# **NOTE — Tier-0 throughput is out of scope.** On the developer
# workstation, the virtio + 1-vCPU-SUT path bottoms out the SUT well
# before reaching the firewall's actual capability. The B-00 number
# here is the harness ceiling, not a SUT measurement. The script is
# kept runnable so the Tier-1 lab box can use it as-is. See
# bench/README.md "Scope decision" for details.
#
# Usage: b00.sh <candidate> <result-dir> <harness-key> <trex-ip> <sink-ip>
set -euo pipefail

CAND="${1:?candidate required}"
RESULT_DIR="${2:?result dir required}"
HARNESS_KEY="${3:?harness key path required}"
TREX_LAN_IP="${4:?trex LAN IP required}"
SINK_WAN_IP="${5:?sink WAN IP required}"

BENCH_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SUT_LAN_IP=10.10.0.1
RAW_LOG="$RESULT_DIR/raw.log"
RESULT_JSON="$RESULT_DIR/result.json"
mkdir -p "$RESULT_DIR"
: >"$RAW_LOG"

log() { printf '[b-00] %s\n' "$*" | tee -a "$RAW_LOG"; }
trex_ssh() {
  ssh -i "$HARNESS_KEY" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
      -o LogLevel=ERROR -o ConnectTimeout=10 bench@"$TREX_LAN_IP" "$@"
}
sut_ssh() {
  ssh -i "$HARNESS_KEY" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
      -o LogLevel=ERROR -o ConnectTimeout=10 bench@"$SUT_LAN_IP" "$@"
}
sink_ssh() {
  ssh -i "$HARNESS_KEY" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
      -o LogLevel=ERROR -o ConnectTimeout=10 bench@"$SINK_WAN_IP" "$@"
}

log "candidate=$CAND result-dir=$RESULT_DIR trex=$TREX_LAN_IP sink=$SINK_WAN_IP"

# --- Wait for SSH on all three VMs -------------------------------------------
log "waiting for SUT + TRex + sink SSH ..."
# Sink lives on the WAN bridge; the host can reach it via thur-wan0 (203.0.113.254).
for tgt in "$SUT_LAN_IP" "$TREX_LAN_IP" "$SINK_WAN_IP"; do
  for i in {1..60}; do
    if ssh -i "$HARNESS_KEY" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
       -o LogLevel=ERROR -o ConnectTimeout=3 bench@"$tgt" true 2>/dev/null; then
      log "  $tgt: reachable after ${i}s"; break
    fi
    sleep 2
    [ "$i" = 60 ] && { log "  $tgt: NOT REACHABLE — aborting"; exit 1; }
  done
done

# --- Wait for cloud-init on all VMs -----------------------------------------
log "waiting for cloud-init to finish on SUT + TRex + sink ..."
sut_ssh  "cloud-init status --wait" >>"$RAW_LOG" 2>&1 || true
trex_ssh "cloud-init status --wait" >>"$RAW_LOG" 2>&1 || true
sink_ssh "cloud-init status --wait" >>"$RAW_LOG" 2>&1 || true

# --- Verify SUT ruleset state ------------------------------------------------
# B-00 wants pass-through. The nftables candidate's stock rules.nft has
# `policy drop` on forward, so for B-00 we either flush rules or rely
# on the candidate having a pass-through mode. Easiest: flush ruleset
# on the SUT just for the duration of B-00.
log "B-00: flushing SUT ruleset for the duration of the test (pass-through)"
sut_ssh "sudo nft flush ruleset && sudo sysctl -w net.ipv4.ip_forward=1" >>"$RAW_LOG" 2>&1

# --- Re-resolve TRex MACs after SUT is up -----------------------------------
log "re-running trex-prepare so it learns the SUT's MACs"
trex_ssh "sudo systemctl restart trex-prepare.service && sudo systemctl restart trex-server.service" >>"$RAW_LOG" 2>&1
sleep 5  # give TRex a moment to bind sockets

# --- Wait for TRex RPC to come up -------------------------------------------
log "waiting for TRex RPC on $TREX_LAN_IP:4501 ..."
for i in {1..30}; do
  if trex_ssh "ss -tnlp 2>/dev/null | grep -q ':4501'" 2>/dev/null; then
    log "  TRex RPC: up after ${i}s"; break
  fi
  sleep 2
  [ "$i" = 30 ] && { log "  TRex RPC: NOT UP — aborting"; trex_ssh 'sudo journalctl -u trex-server.service -n 50 --no-pager' >>"$RAW_LOG" 2>&1 || true; exit 1; }
done

# --- Drive the benchmark ---------------------------------------------------
# Copy the Python driver + harness SSH key to TRex VM. The driver needs
# the key to SSH to the sink VM and read its RX counter each trial.
log "uploading b00.py + harness key to TRex VM"
scp -i "$HARNESS_KEY" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
    -o LogLevel=ERROR \
    "$BENCH_DIR/scripts/b00.py" bench@"$TREX_LAN_IP":/tmp/b00.py >>"$RAW_LOG" 2>&1
trex_ssh "mkdir -p ~/.ssh && chmod 700 ~/.ssh" >>"$RAW_LOG" 2>&1
scp -i "$HARNESS_KEY" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
    -o LogLevel=ERROR \
    "$HARNESS_KEY" bench@"$TREX_LAN_IP":.ssh/harness >>"$RAW_LOG" 2>&1
trex_ssh "chmod 600 ~/.ssh/harness" >>"$RAW_LOG" 2>&1
# Pre-warm ARP for the sink from the TRex VM so its first ssh isn't slow.
trex_ssh "ssh -i ~/.ssh/harness -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=10 bench@$SINK_WAN_IP true" >>"$RAW_LOG" 2>&1 || true

log "running B-00 (this will take several minutes)"
B00_JSON=$(trex_ssh "SINK_IP=$SINK_WAN_IP python3 /tmp/b00.py" 2>>"$RAW_LOG")
echo "$B00_JSON" >"$RESULT_DIR/trex-raw.json"

# --- Wrap in our standard envelope -----------------------------------------
HOST_KERNEL=$(uname -srv)
HOST_CPU=$(grep -m1 '^model name' /proc/cpuinfo | sed -e 's/^[^:]*: //')

python3 - "$B00_JSON" "$CAND" "$HOST_KERNEL" "$HOST_CPU" >"$RESULT_JSON" <<'PY'
import json, sys
trex = json.loads(sys.argv[1])
out = {
    "schema_version": 1,
    "result_type": "b-00",
    "candidate": sys.argv[2],
    "tier": "0-workstation-virtio",
    "generator": "trex-software-af_packet",
    "search": "binary NDR/PDR (MLRsearch-flavoured)",
    "host_invariants": {
        "kernel": sys.argv[3],
        "cpu_model": sys.argv[4],
        "numa_pinning": "none",
        "hugepages": "guest-only",
        "nic": "virtio-net",
        "queue_count": 1,
    },
    "frame_sizes_b": trex["frame_sizes_b"],
    "per_size": trex["per_size"],
    "notes": [
        "B-00 / reference path control (tests/benchmarks.md § 0.6).",
        "Tier-0 numbers characterise the virtio path, not the SUT envelope.",
        "TRex runs in --software (af_packet) mode; no DPDK pmd binding.",
        "Trial duration is shorter than § 0.11 (60s/trial) to keep the harness usable on a workstation; longer trials are a tuning knob in scripts/b00.py.",
    ],
}
json.dump(out, sys.stdout, indent=2)
PY

log "wrote $RESULT_JSON"
log "OK"
