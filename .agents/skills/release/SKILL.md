---
name: release
description: Use when preparing a version bump, release commit, or git tag in this repository. Covers version consistency, CHANGELOG and ROADMAP updates, release builds, and the tag and push checklist.
---

# Release Workflow

`AGENTS.md` is the canonical rule set. A release commit is still an atomic
commit, but it intentionally groups the version and release metadata changes.

## 1. Choose the version

- Follow Semantic Versioning.
- Confirm the next version with the user when the bump type is not explicit.
- Check existing tags with `git tag --sort=-v:refname`.

## 2. Update every version source

- `Cargo.toml` `package.version` is the runtime source of truth; the MCP
  `serverInfo` and `/api/version` read `CARGO_PKG_VERSION`.
- Update `Cargo.lock` by building after the version change.
- Update `CHANGELOG.md` using Keep a Changelog sections (`Added`, `Changed`,
  `Fixed`, `Security`) and the release date.
- Update `ROADMAP.md` status or commit references only when the roadmap tracks
  the released work.
- Search for stale version strings in README and workflow files:
  `rg -n "0\.[0-9]+\.[0-9]+|v[0-9]+\.[0-9]+\.[0-9]+"`.
- Keep artifact naming in `.github/workflows/release.yml` derived from the tag;
  do not hardcode a new version there.

## 3. Verify the release

Run the full gate, then the release build CI uses:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --all-targets --all-features
cargo build --all-targets --all-features
cargo build --release --all-targets --all-features
```

Also run `node --test src/views/pagination.test.js` when JavaScript changed.

## 4. Commit and tag

- Commit with `chore(release): prepare vX.Y.Z`.
- Do not add co-author or AI attribution trailers.
- Tag only after the release commit is on `main`: `git tag vX.Y.Z`.
- Push only when the user asks: `git push origin main --tags`.
- Confirm the tag target with `git show --stat --oneline vX.Y.Z`.

## 5. Do not

- Do not bump versions in unrelated commits.
- Do not rewrite, move, or force-push an existing tag.
- Do not publish a release while the quality gate is failing.
- Do not commit `.env`, `config.json`, release binaries, or generated
  artifacts.
