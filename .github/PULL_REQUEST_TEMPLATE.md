<!--
Thanks for the PR. Keep it small and focused. The PR title must be a
Conventional Commits message (the CI conventional-commits job rejects
non-conformant titles).
-->

## Summary

<!-- One paragraph. What changes and why. -->

## Linked ADR / chapter

<!-- If this PR changes behaviour, the architecture docs come first.
Link the ADR (`docs/architecture/decisions/NNNN-...md`) or chapter
revision that authorises the change. If the change is architectural
but no ADR exists yet, open the ADR first. -->

## Test plan

<!-- How a reviewer can verify the change.
For docs-only PRs: which `tests/scenarios.md` or `tests/benchmarks.md`
entries does this affect, even if only by reference?
For code PRs (once src/ exists): tests added/updated. -->

## Checklist

- [ ] PR title follows Conventional Commits
- [ ] Architecture docs / ADR updated (if behaviour changed)
- [ ] `examples/rules.yaml` still validates against `schemas/rules.schema.json`
- [ ] `tests/scenarios.md` updated (if observable behaviour changed)
- [ ] `tests/benchmarks.md` updated (if performance envelope changed)
- [ ] No secrets, credentials, or private endpoints in the diff
