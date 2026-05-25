#!/usr/bin/env python3
# b00.py — runs INSIDE the TRex VM. Drives TRex's stateless API to
# perform a binary-search NDR/PDR sweep across the seven RFC 2544
# frame sizes (B-00 reference path per tests/benchmarks.md § 0.6).
#
# Topology: TRex single-port TX on enp1s0, packets dest=$SINK_IP.
# RX accounting reads /sys/class/net/enp1s0/statistics/rx_packets on
# the sink VM via SSH at start and end of each trial. Loss is
# (tx - rx_delta) / tx.
#
# Output: JSON on stdout — picked up by scripts/b00.sh.
import json
import os
import subprocess
import sys
import time

TREX_CLIENT_LIB = "/opt/trex/current/automation/trex_control_plane/interactive"
sys.path.insert(0, TREX_CLIENT_LIB)

from trex_stl_lib.api import STLClient, STLStream, STLPktBuilder, STLTXCont, STLError  # noqa: E402
from scapy.layers.inet import IP, UDP                                                   # noqa: E402
from scapy.layers.l2 import Ether                                                       # noqa: E402

RFC2544_FRAMES = [64, 128, 256, 512, 1024, 1280, 1518]
TRIAL_SEC      = 10        # § 0.11 calls for 60s; tunable down for iteration speed
SEARCH_ITER    = 8
PDR_THRESHOLD  = 0.005     # 0.5% drop tolerance for PDR
# Ceiling for the binary search. Set to a realistic Tier-0 upper bound
# (SW-mode TRex + 1-vCPU SUT VM with virtio caps well below 1 Mpps).
# Set too high and the search wastes all its iterations above the actual
# NDR and reports 0; set too low and we cap the result artificially.
RATE_HIGH_PPS  = 500_000

SRC_IP   = "10.10.0.200"
SINK_IP  = os.environ.get("SINK_IP", "203.0.113.200")
SRC_PORT = 1024
DST_PORT = 5000

# How to read the sink VM's RX packet counter.
SINK_KEY = "/home/bench/.ssh/harness"     # uploaded by b00.sh
SINK_SSH_OPTS = ["-i", SINK_KEY, "-o", "StrictHostKeyChecking=no",
                 "-o", "UserKnownHostsFile=/dev/null", "-o", "LogLevel=ERROR",
                 "-o", "ConnectTimeout=5"]


def sink_rx_packets():
    """Read /sys/class/net/enp1s0/statistics/rx_packets from the sink VM."""
    cmd = ["ssh", *SINK_SSH_OPTS, f"bench@{SINK_IP}",
           "cat /sys/class/net/enp1s0/statistics/rx_packets"]
    out = subprocess.check_output(cmd, text=True, timeout=10).strip()
    return int(out)


def make_stream(frame_size_b):
    pad_len = max(0, frame_size_b - 4 - 14 - 20 - 8)  # FCS(4)+ETH(14)+IP(20)+UDP(8)
    base_pkt = Ether() / IP(src=SRC_IP, dst=SINK_IP) / UDP(sport=SRC_PORT, dport=DST_PORT) / ("x" * pad_len)
    pkt = STLPktBuilder(pkt=base_pkt)
    return STLStream(packet=pkt, mode=STLTXCont(pps=1))  # rate set via mult= at start


def run_trial(c, frame_size_b, rate_pps, duration_s):
    c.reset(ports=[0])
    stream = make_stream(frame_size_b)
    c.add_streams(stream, ports=[0])
    c.clear_stats()
    rx_before = sink_rx_packets()
    c.start(ports=[0], mult=f"{rate_pps}pps", duration=duration_s, force=True)
    c.wait_on_traffic(ports=[0], timeout=duration_s + 30)
    # Sink counter updates may lag a hair after the last frame; small sleep.
    time.sleep(0.5)
    rx_after = sink_rx_packets()
    stats = c.get_stats()
    tx = stats[0].get("opackets", 0)
    rx = max(0, rx_after - rx_before)
    if tx == 0:
        return tx, rx, 1.0, 0.0
    loss_pct = max(0.0, min(1.0, (tx - rx) / tx))
    achieved_mpps = (tx / duration_s) / 1e6
    return tx, rx, loss_pct, achieved_mpps


def search_one_size(c, frame_size_b):
    lo, hi = 0, RATE_HIGH_PPS
    ndr_pps = 0
    pdr_pps = 0
    history = []

    for i in range(SEARCH_ITER):
        mid = (lo + hi) // 2
        if mid < 10_000:
            break
        tx, rx, loss, mpps = run_trial(c, frame_size_b, mid, TRIAL_SEC)
        history.append({"iter": i + 1, "rate_pps": mid, "tx": tx, "rx": rx,
                        "loss_pct": loss, "achieved_mpps": mpps})
        if loss == 0:
            ndr_pps = max(ndr_pps, mid)
            lo = mid
        elif loss <= PDR_THRESHOLD:
            pdr_pps = max(pdr_pps, mid)
            lo = mid
        else:
            hi = mid

    def gbps(pps, size_b):
        return (pps * size_b * 8) / 1e9

    return {
        "frame_size_b": frame_size_b,
        "ndr_pps": ndr_pps,
        "ndr_gbps": gbps(ndr_pps, frame_size_b),
        "pdr_pps": pdr_pps,
        "pdr_gbps": gbps(pdr_pps, frame_size_b),
        "history": history,
    }


def main():
    c = STLClient(server="127.0.0.1")
    try:
        c.connect()
        c.reset(ports=[0])
        c.set_port_attr(ports=[0], promiscuous=True)

        # Sanity-probe the sink RX path before kicking off the sweep.
        rx0 = sink_rx_packets()
        print(f"sink RX counter at start: {rx0}", file=sys.stderr, flush=True)

        results = []
        for fs in RFC2544_FRAMES:
            t0 = time.time()
            r = search_one_size(c, fs)
            r["elapsed_s"] = round(time.time() - t0, 1)
            results.append(r)
            print(f"  frame={fs}B NDR={r['ndr_pps']:>10} pps ({r['ndr_gbps']:.3f} Gbps)  "
                  f"PDR={r['pdr_pps']:>10} pps ({r['pdr_gbps']:.3f} Gbps)  in {r['elapsed_s']}s",
                  file=sys.stderr, flush=True)

        out = {
            "frame_sizes_b": RFC2544_FRAMES,
            "trial_duration_s": TRIAL_SEC,
            "search_iterations": SEARCH_ITER,
            "pdr_threshold": PDR_THRESHOLD,
            "rx_source": f"sink:{SINK_IP}:enp1s0",
            "per_size": results,
        }
        json.dump(out, sys.stdout)
    except STLError as e:
        print(f"TRex error: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        try:
            c.disconnect()
        except Exception:
            pass


if __name__ == "__main__":
    main()
