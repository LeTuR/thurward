# `candidates/trafficgen/` — Tier-0 iperf3 + ping generator

A single Debian 12 VM that hosts **both** endpoints of the smoke test,
one on each side of the firewall:

```
            ┌──────────────────── thur-gen ────────────────────┐
            │                                                  │
LAN side    │  root netns                       wan netns      │  WAN side
10.10.0.100 │  enp1s0 ── 10.10.0.100/24  enp2s0 ── 203.0.113.100│  203.0.113.100
            │           default via                default via │
            │           10.10.0.1  ───┐         ┌── 203.0.113.1│
            │                         │         │              │
            └─────────────────────────┼─────────┼──────────────┘
                                      ▼         ▲
                              ┌─────────────────────┐
                              │  thur-sut-<cand>    │   the firewall under test
                              │  10.10.0.1   →  fwd │
                              │  203.0.113.1 ←      │
                              └─────────────────────┘
```

## Why a netns?

Without the split, both `10.10.0.100` and `203.0.113.100` would be
local addresses in the same kernel namespace. An `iperf3 -c
203.0.113.100` started from the same VM would be short-circuited via
loopback by the kernel — and quietly bypass the firewall entirely. The
smoke test would then report a passing run that proved nothing.

Putting `enp2s0` into a `wan` network namespace removes
`203.0.113.0/24` from the root netns's address table. The only route
left from root to WAN-side addresses is the default route via
`10.10.0.1` — i.e. through the SUT. That is what guarantees the
FORWARD chain is actually exercised.

## Files

- `cloud-init/user-data` — declares the netns helper script, the
  systemd unit that runs it, and the `iperf3-server@.service` template
  that execs into the netns.
- `cloud-init/meta-data` — boilerplate (instance-id, hostname).

## What runs at boot

1. `netplan apply` — addresses `enp1s0` only.
2. `trafficgen-wan-ns.service` — moves `enp2s0` into the `wan` netns,
   assigns `203.0.113.100/24`, installs the default route via
   `203.0.113.1`.
3. `iperf3-server@5201.service` — enabled at boot; the unit `ExecStart`
   wraps `ip netns exec wan iperf3 -s`, so the server listens inside the
   netns.

## Useful from inside the VM

```
sudo /usr/local/sbin/trafficgen-wan-ns status   # see wan-netns addrs/routes
sudo ip netns exec wan ss -lntp                 # see wan-side listeners
sudo ip netns exec wan tcpdump -ni enp2s0       # capture on the WAN side
```
