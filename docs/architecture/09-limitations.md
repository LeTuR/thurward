# 09 — Limitations

*Audience: anyone evaluating whether thurward fits their use case.
Read before adopting.*

These are intentional, known, and documented. If any are deal-breakers
for your environment, thurward is not the right choice for you today.

## No high availability (v1)

v1 ships **single-VM / single-box**. If the firewall crashes, default
deny holds — every flow drops until the launcher restarts the unit.
With NAT in scope ([ADR 0014](decisions/0014-stateful-nat.md)), a
crash means **every NAT binding is lost**, so even established flows
break and clients must reconnect.

The reference systemd unit on Path B
([06 — Deployment](06-deployment.md)) sets `Restart=always`, so
transient crashes recover within seconds — but the outage window is
real.

HA is deferred to v2 ([ADR 0015](decisions/0015-active-passive-ha.md)
is preserved as a starting point but reverted from v1). v2 will pick
a mechanism that doesn't depend on the controller machinery v1 has
removed.

If sub-second failover with zero flow loss is a hard requirement for
your environment today, thurward v1 is not for you.

## NAT scope

thurward implements **SNAT/masquerade + static DNAT** only
([ADR 0014](decisions/0014-stateful-nat.md)). The following NAT
features are **out of scope for v1**:

- **Hairpin NAT.** A LAN client connecting to its own WAN IP will not
  loop through DNAT correctly. Workaround: connect via the internal
  IP directly, or use split-horizon DNS.
- **ALGs** (FTP, SIP, H.323, PPTP, IRC DCC). Protocols that embed IP
  addresses inside payloads will not work across NAT.
- **CGNAT-scale port allocation.** thurward uses a deterministic
  per-tuple SNAT port pool; it is sized for an edge deployment, not
  for thousands of subscribers behind one IP.
- **NAT64 / NAT66 / DNS64.** IPv6 itself is unsupported (below);
  translation between address families is therefore moot.

## Throughput at 10G+ requires multi-queue (v1 ships single-queue)

The v1 fast path runs **one RX poll thread per interface**
([ADR 0013](decisions/0013-zig-fast-path-on-uknetdev.md)). Realistic
target on a single vCPU with virtio-net is **~1 Gbps sustained**
(comfortable for edge / branch / SOHO deployments). Pushing into the
5–10 Gbps band requires virtio-net multi-queue + RSS with per-thread
conntrack shards, which is a documented v1.x stretch goal — not
shipping in v1.

If you need 10G+ on day one, thurward v1 is not for you; pick something
DPDK-based or run a Linux+XDP/eBPF appliance.

## IPv4 only (v1)

IPv6 is architected for a future version but not implemented. v6
packets are dropped unconditionally. If you have a v6-native
environment, thurward v1 is not for you.

## No bare-metal direct boot (v1)

v1 targets are **QEMU/KVM** and **Firecracker** only
([06 — Deployment](06-deployment.md)). Bare-metal x86_64 EFI direct
boot is deferred to v2 — per
[ADR 0017](decisions/0017-hermit-rust-substrate.md) and
[ADR 0018](decisions/0018-substrate-pivot-unikraft.md), both
substrate-related decisions explicitly keep bare-metal out of v1
scope. Unikraft does have a credible bare-metal direct-boot story,
but v1 does not exercise it.

If you need a dedicated-hardware deployment today, run a small
Linux+KVM appliance image on the box and launch Firecracker (Path B)
on top.

The v2 ADR for bare-metal will revisit the trade-off: Unikraft's
bare-metal target may be ready to lift, or a Linux+KVM appliance
image may remain the pragmatic answer, or a different unikernel's
bare-metal story may have caught up (MirageOS, etc.).

ARM in general is also not in v1.

## No hot rule reload

Every rule change is a new image and a launcher restart. End-to-end
latency: order of seconds, not milliseconds. This is a *feature* under
the security posture
([ADR 0016](decisions/0016-image-per-change-user-deploy.md)) — the
firewall has no runtime admin surface — but it changes how operators
think about urgent fixes.

There is no break-glass mechanism on the running firewall. Emergency
procedure: roll back to the previous image (Path B/C kept the prior
artifact, right?) and `systemctl restart` / reboot.

## No fleet rollout

v1 expects a human (or that human's configuration management of choice
— Ansible, Puppet, manual SSH) to run the install on each target host.
There is no "deploy this change to every firewall in the fleet"
mechanism shipped with the project. The build-and-publish side
(`cosign sign`, SLSA attestation, push to registry) is a CI workflow;
the consume side is local to each host.

If you operate many thurward instances, you wire your own fleet
mechanism around the documented install commands. The reference
material doesn't presume one.

## DNS bypass

Clients that hardcode `8.8.8.8`, use DoH, or use DoT to a remote
resolver bypass thurward's DNS proxy entirely and therefore bypass
FQDN policy. Mitigation is operational (block other DNS endpoints on
the LAN, transparent UDP/53 NAT to thurward); it cannot be solved
inside thurward.

## No L7 inspection

thurward filters on the 5-tuple and FQDN binding. Once a connection
is allowed, traffic flowing over it is not inspected. If you need
TLS inspection or WAF behaviour, run a separate appliance.

## Restart loses fqdn_set

On boot, `fqdn_set` is empty until clients re-resolve. Without HA
([above](#no-high-availability-v1)), a restart means a cold cache;
FQDN-rule matches re-warm as clients re-query. The optional warm-replay
mechanism documented in [07 — Operations](07-operations.md) mitigates
this for environments that need it.

## Vsock requirement on hypervisor hosts

For Path A (QEMU) and Path B (Firecracker), the host kernel must have
`vhost_vsock` loaded (`modprobe vhost_vsock`). Without it, the
observability channel doesn't open.

## `lib-rust` on Unikraft is early-adopter territory

Unikraft's Rust integration (`lib-rust`) is less mature than its C
support. Some Rust crates that assume a hosted runtime (`std::*`) may
not build cleanly inside the image — the hot loop is already
`no_std`-friendly per [ADR 0013](decisions/0013-zig-fast-path-on-uknetdev.md),
but adding new runtime dependencies needs a `no_std` check. Build-
time crates run on the host and aren't subject to this constraint.
Upstream patches we contribute to `lib-rust` may take longer to land
than they would in Unikraft's C ecosystem.

## Toolchain pinning is heavier than the prior Zig stack

The Rust + Unikraft + smoltcp stack pins more moving parts than the
prior Zig stack did: `rust-toolchain.toml` (rustup channel +
components), `Cargo.lock` (every transitive crate), the Unikraft
revision plus each selected `lib-*` component revision, and the
smoltcp crate version all live in `versions.lock` (see
[ADR 0010](decisions/0010-supply-chain-hardening.md) and
[ADR 0018](decisions/0018-substrate-pivot-unikraft.md)). Upgrades
are deliberate, reviewed actions with reproducibility re-validation
— not passive `cargo update` runs.

## Out of scope explicitly

- **HA in v1** — deferred to v2 (above).
- **Bare-metal direct boot in v1** — deferred to v2 (above).
- **Hairpin NAT, ALGs, CGNAT, NAT64/66** — see "NAT scope" above.
- **DPDK / AF\_XDP / multi-queue RSS** in v1 — stretch goal per
  [ADR 0013](decisions/0013-zig-fast-path-on-uknetdev.md).
- **ARM (any target)** — later ADR.
- **Custom Terraform provider** — retired with
  [ADR 0012](decisions/0012-reference-terraform-modules.md).
- **Deploy controller / k8s operator** — retired with
  [ADR 0016](decisions/0016-image-per-change-user-deploy.md).
- **GCP deployment-target diagrams** — drawio skill doesn't ship
  reliable GCP icons; use Azure or generic catalog instead.
- **DNSSEC validation, DoH/DoT termination, ECS subnet forwarding** —
  see [04 — FQDN & DNS](04-fqdn-and-dns.md) "out of scope" section.
