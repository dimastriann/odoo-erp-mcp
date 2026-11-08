---
name: atomic-commit
description: Use when staging, committing, or reviewing changes in this repository. Enforces atomic commits, the required Rust quality gate (fmt, clippy, tests, debug build), and concise Conventional Commit messages without co-author attribution.
---

# Atomic Commit Workflow

`AGENTS.md` is the canonical rule set. This skill is the step-by-step procedure
for turning a working tree into small, verified commits.

## 1. Inspect before touching git

```powershell
git status --short
git diff --stat
git log --oneline -10
```

Read the changed files and group them into independent logical units. Each unit
must have one purpose, include its tests, and be revertable on its own.

## 2. Split mixed work

If the tree contains unrelated changes, do not commit everything at once. Create
one commit per unit and commit small coherent increments rather than batching.
Do not silently discard or stash changes the user did not ask to touch.

## 3. Stage explicit paths

```powershell
git add -- path/to/file path/to/test
git diff --cached
```

Never use a blind `git add -A` or `git add .`. Never stage `.env`, `config.json`,
`target/`, credentials, or generated artifacts.

## 4. Run the quality gate

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --all-targets --all-features
cargo build --all-targets --all-features
```

Run `cargo fmt --all` to apply formatting fixes, then rerun the full gate. Do not
commit while any step fails. Do not use `--no-verify`, `#[allow(...)]`, or test
skips to bypass a failure.

When JavaScript helpers are added, also run:

```powershell
node --test src/views/pagination.test.js
```

Before a release or version-tag commit, also run:

```powershell
cargo build --release --all-targets --all-features
```

## 5. Write the commit message

```text
type(scope): imperative summary
```

- Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`, `ci`,
  `build`.
- Keep the subject lowercase, imperative, and at most 72 characters.
- Add a short body (normally 1-4 lines, wrapped at 72) only when it explains
  context, impact, or a trade-off. Do not restate the diff.
- No `Co-authored-by`, AI attribution, or generated-by trailers.

Create the commit with repeated `-m` flags so it works in every shell:

```powershell
git commit -m "type(scope): imperative summary" -m "Optional concise reason."
```

## 6. Verify and continue

```powershell
git show --stat --oneline HEAD
git status --short
```

Confirm the commit contains only the intended files and the tree is clean for
the next unit. Repeat from step 1 for remaining units.

## Stop conditions

- A quality-gate failure unrelated to the current change: report it and do not
  commit a partial result. Fix it in a separate commit only if it is in scope.
- A change touches secrets, credentials, or security defaults: stop and ask
  before committing.
- The user did not ask for a commit: make the changes and leave them uncommitted.
- The user explicitly asks for review before committing: stop and show the diff.
