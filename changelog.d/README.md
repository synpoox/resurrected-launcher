# Changelog fragments

`CHANGELOG.md` is written only by the release PR. A PR that changes what players see adds one fragment file here instead, so PRs stop conflicting on the same lines.

- One file per PR, named `<issue>-<slug>.md`, for example `277-catalog-refresh.md`.
- First line: one heading, `### New`, `### Changed` or `### Fixed`.
- Then a blank line, then one bullet per change, in player wording, ending with the PR number `(#n)`.
- No version or date.

Example:

```
### Fixed

- One product's broken release no longer makes every product show offline. The others keep their updates. (#277)
```

At release, the release PR moves every fragment's bullets under the matching `###` heading of the new `## <version>` section in `CHANGELOG.md`, then deletes the fragments. This README is never folded.
