# ADR 0004 — DNS proxy for FQDN rules

**Status:** Accepted
**Date:** 2026-05-23
**Deciders:** magicletur

## Context

Packet headers carry IP addresses, not domain names. To filter on FQDN
(e.g. "allow outbound HTTPS to `github.com`"), the firewall has to know
which IPs currently belong to which FQDN. The two established approaches:

1. **Pre-resolve and refresh.** A periodic job (cron / agent) resolves
   each rule's FQDNs and feeds the IPs into an allow-set
   (ipset, eBPF map). Simple to implement.
2. **DNS-proxy interception.** The firewall is itself the DNS resolver
   for the protected network. It forwards queries upstream, parses
   responses, and adds `(qname → IP)` mappings to its allow-set with
   the response's TTL.

The user picked DNS-proxy interception.

The relevant pitfalls of pre-resolve+refresh:

- **TTL race**: rule resolves to one IP at refresh time; client resolves
  to a *different* IP a moment later (CDNs do this constantly). Client
  connects to an IP the firewall doesn't know about → drop. Looks like
  the firewall is broken to the user.
- **CDN churn**: aggressive DNS load-balancing (Akamai, Cloudflare,
  Fastly, CloudFront) returns thousands of IPs across short TTLs.
  Pre-resolving at a coarser interval than the TTL guarantees mismatch.

## Decision

Run a DNS proxy inside thurward, bound to UDP/53 (and TCP/53 for
fallback) on the LAN-facing interface. The proxy:

1. Accepts queries from LAN clients.
2. Forwards them to an upstream resolver (configured at build time).
3. Parses A/AAAA responses (including CNAME chains).
4. Inserts each `(qname, ip)` pair into `fqdn_set`, keyed by qname,
   with `expiry = now + response.ttl`.
5. Returns the unmodified response to the client.

`fqdn_set` is checked by the packet filter whenever a rule's match
predicate includes an FQDN — the destination IP must be currently
present under the matching qname.

LAN clients must use thurward as their DNS resolver. Clients that
hardcode `8.8.8.8` or use DoH bypass the policy entirely; this is
listed honestly in [09 — Limitations](../09-limitations.md).

## Consequences

- (+) FQDN rules are *correct under TTL*: clients and the firewall see
  the same resolved IPs because they came from the same response.
- (+) CDN churn is handled by construction: every new CDN IP a client
  learns about, the firewall learned about a moment earlier.
- (+) Per-client visibility: who looked up what is observable in the
  proxy logs.
- (+) DNSSEC validation could be added at the proxy in a future
  milestone without changing the rule path.
- (−) The DNS proxy is on the data path for every name lookup —
  it's a choke point. Misbehaviour (slowness, parse bugs, crashes)
  affects every LAN client.
- (−) Clients that bypass the proxy (DoH, hardcoded `8.8.8.8`) bypass
  FQDN policy. Mitigation is operational (block DoH endpoints,
  redirect UDP/53 transparently if needed) — documented in
  [04 — FQDN & DNS](../04-fqdn-and-dns.md).
- (−) The proxy must handle real-DNS edge cases: UDP truncation (`TC=1`
  → upgrade to TCP), EDNS0 buffer sizes, NXDOMAIN/SERVFAIL passthrough,
  CNAME chains, rate-limiting per source.
- (○) `fqdn_set` size is bounded by `Σ(active TTL × lookup rate)`;
  long-TTL CDNs grow it. Per-client rate-limit caps the worst case.

## Alternatives considered

- **Pre-resolve and refresh.** Simpler, but wrong under any non-trivial
  TTL or CDN, as discussed in Context. The user agreed this is the
  wrong production answer.
- **Snoop DNS traffic without terminating it.** Promiscuous-mode read
  of UDP/53 transit traffic; populate `fqdn_set` from observed responses.
  Less invasive (no proxy in the data path) but doesn't work over
  encrypted DNS (DoT/DoH) and requires the firewall to see the response,
  which is fragile on the inline middlebox where traffic might
  asymmetrically route.
- **Static `/etc/hosts`-style baked allowlist of IPs.** Defeats the
  purpose of FQDN rules and breaks on any cloud-hosted service.
