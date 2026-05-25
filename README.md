# thurward

> A minimalistic open-source firewall — Hermit unikernel, Rust data plane on
> smoltcp wire types, SNAT + DNAT, FQDN-aware, first-class observability.
> Runs on QEMU/KVM or Firecracker. Gigabit-class on a single vCPU; headroom
> for 10G with multi-queue.

## Status

🚧 **Architecture phase.** No implementation code yet.

This repo currently holds *only* design documentation. The architecture is
captured as a set of decision records (ADRs) and topical chapter docs. Once
the architecture is settled, the next phase scaffolds the unikernel itself.

## What it is

- **Inline middlebox with NAT.** Sits between two NICs (LAN ↔ WAN), forwards
  or drops packets per declarative rules, performs SNAT/masquerade for LAN
  egress and static DNAT (port-forward) for inbound services.
- **Rules on source / port / FQDN.** Source CIDR + destination port + DNS
  name (with wildcards). DNS-proxy interception resolves FQDNs honestly,
  per response TTL.
- **Unikernel, Rust data plane.** Built on [Hermit](https://hermit-os.org).
  The entire data path — parsing, conntrack, filtering, NAT, TX scheduling —
  is [Rust](https://rust-lang.org) using `smoltcp::wire` for typed packet
  parsing, with no in-image TCP/IP socket layer between the driver and the
  filter. Boots in milliseconds, ~MB image, no general-purpose OS underneath.
- **Two v1 deployment targets, one image.** Local dev on QEMU/KVM
  (`make run`), VPS / managed host on Firecracker. Bare-metal direct boot
  is deferred to v2. See
  [`docs/architecture/06-deployment.md`](docs/architecture/06-deployment.md).
- **Source-of-truth: one git repo.** Filter rules, SNAT pools, and DNAT
  entries live in `rules.yaml` alongside the firewall source and compile
  into the image at build time. Rule change = edit YAML → `make build` →
  install. **No runtime admin API on the firewall.** No separate "rules
  repo", no deploy controller, no Terraform.
- **User-verifiable supply chain.** Reproducible builds, pinned deps,
  cosign-signed images, SLSA-3 provenance. The operator runs
  `cosign verify` and `slsa-verifier` before installing — there is no
  controller in the loop.
- **Observable.** ECS-aligned JSON drop/accept logs (with NAT translation
  fields), Prometheus metrics with per-rule labels plus conntrack / NAT
  gauges, per-flow OpenTelemetry traces. All egress to a host-side
  collector over virtio-vsock — no in-band telemetry traffic.

**v1 is single-VM.** HA and bare-metal direct boot are both deferred to
v2; if the firewall crashes, default deny holds until restart. See
[`docs/architecture/09-limitations.md`](docs/architecture/09-limitations.md).

## Read the architecture

Start with [`docs/architecture/00-overview.md`](docs/architecture/00-overview.md),
then the [decision records](docs/architecture/decisions/), then the topical
chapters.

| Chapter                                                                            | What it covers                                              |
| ---------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| [00 — Overview](docs/architecture/00-overview.md)                                  | Problem, goals, non-goals, glossary, stack at a glance     |
| [01 — System context](docs/architecture/01-system-context.md)                      | What's outside thurward (clients, upstream DNS, collector) |
| [02 — Packet path](docs/architecture/02-packet-path.md)                            | NIC → uknetdev → parse → conntrack → filter → NAT → TX      |
| [03 — Rule model](docs/architecture/03-rule-model.md)                              | YAML schema, build-time compilation, matching semantics    |
| [04 — FQDN & DNS](docs/architecture/04-fqdn-and-dns.md)                            | DNS-proxy design, fqdn_set cache, TTL handling             |
| [05 — Observability](docs/architecture/05-observability.md)                        | ECS log schema, metric names, tracing strategy, vsock      |
| [06 — Deployment](docs/architecture/06-deployment.md)                              | QEMU/Firecracker, bridge topology, upgrade procedure       |
| [07 — Operations](docs/architecture/07-operations.md)                              | Alerts, dashboards, runbooks, install/rollback             |
| [08 — Security model](docs/architecture/08-security-model.md)                      | Threat model, attack surface, supply-chain hardening       |
| [09 — Limitations](docs/architecture/09-limitations.md)                            | The honest list                                            |

## License

TBD — placeholder until a license is chosen at implementation time.

## Pronunciation & name

`thurward` = `thur` (project brand prefix, matches *thurbox*, *thurkube*,
*thurspace*, *thurarch*) + `ward` (the defensive verb — *to ward off*).
