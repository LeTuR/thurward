# Contributing

thurward is pre-v1 and **architecture-docs-first**: the chapters under
`docs/architecture/` and the ADRs under
`docs/architecture/decisions/` are the source of truth. Code follows
docs, not the other way around. If you want to change behaviour,
update or add an ADR first.

## Workflow

1. **Open an issue** for anything non-trivial. For new design
   decisions, propose an ADR (`docs/architecture/decisions/NNNN-...md`)
   following the existing format.
2. **Branch from `main`**. Branch names: `docs/...`, `ci/...`,
   `feat/...`, `fix/...`, `refactor/...`, `test/...`.
3. **Write commits in Conventional Commits format**
   (<https://www.conventionalcommits.org/>). CI rejects non-conformant
   commit messages on PRs. Examples:
   - `docs(adr): 0018 conntrack table sizing`
   - `ci: add ajv-cli schema validation`
   - `fix(dns): honour final-leaf TTL across CNAME chain`
4. **Open a PR** against `main`. PRs are squash-merged.

## What CI checks (today)

The repo is still docs-first; CI runs four jobs on every PR:

- **markdownlint** — all `.md` files
- **schema validation** — `examples/rules.yaml` must validate against
  `schemas/rules.schema.json`
- **link check** — broken internal links fail; broken external links
  warn
- **conventional commits** — PR commit history is verified

When `src/` lands, the Rust pipeline (check / fmt / clippy / nextest /
deny / docs) joins these — modelled on the `thurbox` repo's `ci.yml`.

## What goes in a PR

- One logical change per PR.
- Tests / scenarios updated (`tests/scenarios.md` for behavioural
  changes, `tests/benchmarks.md` for performance-relevant changes).
- ADR added or updated if the change is architectural.
- README / chapter docs updated if the change is user-visible.

## Reporting a security issue

Don't. Use the private process in
[`SECURITY.md`](SECURITY.md) instead of a public issue or PR.

## License

By contributing, you agree your contributions are licensed under the
MIT License (see [`LICENSE`](LICENSE)).
