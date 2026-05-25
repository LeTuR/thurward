# 06 — Deployment

*Audience: anyone going from "I have the firewall repo" to "thurward is
filtering my traffic with my rules." Read after [00 — Overview](00-overview.md)
and the [decision records](decisions/).*

The deployment model is dictated by:

- [ADR 0017](decisions/0017-hermit-rust-substrate.md) — Hermit
  unikernel + Rust; the same image runs on two v1 targets (QEMU/KVM
  and Firecracker). Bare-metal direct boot is deferred to v2 — see
  [09 — Limitations](09-limitations.md).
- [ADR 0013](decisions/0013-zig-fast-path-on-uknetdev.md) — the app
  owns the data path; no in-image TCP/IP socket layer (smoltcp's
  wire/parser modules only).
- [ADR 0016](decisions/0016-image-per-change-user-deploy.md) — the user
  builds the image and installs it. There is no deploy controller, no
  separate rules repo, no automated fleet rollout.

## The big picture

```
    1. clone firewall repo                3. install on target
   ┌──────────────────────┐             ┌────────────────────────┐
   │ git clone ...        │             │ verify image           │
   │ edit rules.yaml      │             │   cosign verify ...    │
   │ edit src/ (optional) │   2. build  │   slsa-verifier ...    │
   │                      │ ──────────► │ stop old image         │
   │ make build           │             │ start new image        │
   │   → thurward.image   │             │   (per launcher below) │
   └──────────────────────┘             └────────────────────────┘
```

Steps 1 and 2 happen on a developer/operator workstation (or in CI if
preferred). Step 3 happens on the target host. The artifact crossing
the boundary is **one signed Hermit unikernel image plus its cosign
signature and SLSA-3 attestation**
([ADR 0010](decisions/0010-supply-chain-hardening.md)).

Both v1 deployment paths consume the same image; only the launcher
differs. Bare-metal direct boot is **not a v1 target** — see
[09 — Limitations](09-limitations.md).

## Path A — Local dev / hobby (QEMU)

The fastest path from clone to running. Useful for trying thurward,
developing rules, or homelab deployments.

**Host prerequisites:** Linux with KVM enabled, the Rust toolchain
(channel pinned in `rust-toolchain.toml`), `qemu-system-x86_64`,
two Linux bridges (`br-lan`, `br-wan`).

```bash
git clone https://github.com/magicletur/thurward
cd thurward
# edit rules.yaml — VS Code / Helix will pull schemas/rules.schema.json
# automatically for autocomplete + inline validation
$EDITOR rules.yaml
make build               # wraps `cargo build --release --target x86_64-unknown-hermit`
make run                 # wraps the QEMU invocation below
```

`make run` invokes:

```bash
qemu-system-x86_64 \
  -enable-kvm -cpu host -smp 1 -m 128M \
  -kernel target/x86_64-unknown-hermit/release/thurward \
  -netdev bridge,id=lan,br=br-lan -device virtio-net-pci,netdev=lan \
  -netdev bridge,id=wan,br=br-wan -device virtio-net-pci,netdev=wan \
  -device vhost-vsock-pci,guest-cid=3
```

The VM gets `10.0.0.1` on the LAN bridge; LAN clients use it as
default gateway and DNS resolver.

No image signing or verification is performed in this path — it's the
hobbyist's local box. If you want signing, follow Path B.

## Path B — VPS / managed host (Firecracker)

Production-shape security posture on a single Linux host without
introducing custom infrastructure.

**Host prerequisites:** Linux with KVM, `vhost_vsock` loaded,
`firecracker` binary, `cosign`, `slsa-verifier`, the two bridges.

The build typically happens in CI rather than on the target host, so
the artifact, signature, and attestation arrive at the target via a
registry (OCI), a GitHub release, or whatever transport you prefer.
The install step on the target:

```bash
# verify before installing
cosign verify --key cosign.pub thurward.image
slsa-verifier verify-artifact thurward.image \
    --provenance-path thurward.image.intoto.jsonl \
    --source-uri github.com/magicletur/thurward \
    --source-branch main

# install (atomic move of the image into the launcher's expected path)
sudo install -m 0644 thurward.image /var/lib/thurward/current.image

# swap the running unit
sudo systemctl restart thurward
```

A reference `thurward.service` systemd unit is shipped in `contrib/`:
it runs `firecracker` with two virtio NICs, one virtio-vsock device
(CID 3, host port 9000 for observability), and the current image; it
restarts on failure. On rule changes, replace `current.image` and
`systemctl restart thurward` — the restart is fast (Firecracker
sub-10ms boot + Hermit sub-10ms boot), but **flows in progress drop**
because v1 has no HA ([09 — Limitations](09-limitations.md)).

## Bare-metal — deferred to v2

A previous revision of this chapter described a third path (bare-metal
x86_64 EFI direct boot). [ADR 0017](decisions/0017-hermit-rust-substrate.md)
walks that back: Hermit's bare-metal story isn't ready for the v1
promise. Bare-metal is a known v2 direction; see
[09 — Limitations](09-limitations.md) for the trade-off and the v2
options under consideration.

If you need a dedicated-hardware deployment today, run Path B on a
small Linux+KVM box (Alpine, Debian-minimal). The image and the
operational story are the same.

## Routing and addressing (both paths)

- **LAN side** — thurward gets `10.0.0.1` (configurable in
  `rules.yaml` defaults). LAN clients use this as both default
  gateway and DNS resolver.
- **WAN side** — thurward gets a static IP appropriate to the
  upstream segment. NAT SNAT'd egress uses this IP (or whichever
  `external_ip` you pin in `rules.yaml`'s `nat.snat[]`).
- **No DHCP server in thurward.** Provide one elsewhere on `br-lan`
  (host-side dnsmasq, an upstream box, etc.) or assign LAN clients
  statically.

## What lives where

| Lives on the host          | Lives in the image                                                                     |
| -------------------------- | -------------------------------------------------------------------------------------- |
| Linux bridges              | Hermit unikernel core                                                                  |
| TAP devices                | Hermit virtio-net + virtio-vsock drivers                                               |
| `vhost_vsock` module       | smoltcp (wire/parser layer only)                                                       |
| `cosign` + `slsa-verifier` | Rust data plane (parse, conntrack, filter, NAT, TX)                                    |
| Collector stack            | Compiled rule + DNAT table (Rust module generated by `build.rs` from rules.yaml)       |
| Launcher unit              | DNS proxy + fqdn_set                                                                   |

The image has no persistent storage. `fqdn_set` and the conntrack
table are reconstructed at boot from observed traffic. Without HA
([09 — Limitations](09-limitations.md)) there's no warm-handoff
target for either.

## What this chapter does not cover

- Rule authoring details — see [03 — Rule model](03-rule-model.md).
- The full alerting/dashboards story — see
  [05 — Observability](05-observability.md) and
  [07 — Operations](07-operations.md).
- Why no GitOps controller — see
  [ADR 0016](decisions/0016-image-per-change-user-deploy.md).
- HA — there is none in v1. See
  [09 — Limitations](09-limitations.md).
