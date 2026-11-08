# Agent Workflow

This repository is an MCP gateway for Odoo. Changes must preserve the typed
error contract, authorization policy, secret redaction, mutation lifecycle, and
query-protection invariants.

## Cross-agent entry points

- `AGENTS.md` is the canonical rule set for Codex, opencode, and Claude Code.
- `CLAUDE.md` imports this file for Claude Code.
- Canonical skills live in `.agents/skills/<name>/SKILL.md`; Codex discovers
  them natively.
- opencode and Claude Code use small pointer skills in
  `.opencode/skills/<name>/SKILL.md` and `.claude/skills/<name>/SKILL.md` that
  redirect to the canonical file. Keep pointer frontmatter in sync with the
  canonical skill's `name` and `description`.
- Reusable procedures: `atomic-commit`, `mcp-tool-extension`,
  `mutation-lifecycle`, `release`, `security-review`, `odoo-compat`, `web-ui`.

## Atomic commits

- Commit every coherent increment, even a small one. Do not batch unrelated
  work into a single commit.
- One logical purpose per commit. Do not mix refactoring, features, UI work, or
  documentation that are not required by that purpose.
- Tests for a behavior belong in the same commit as the behavior.
- Do not create WIP, empty, test-later, or formatting-only commits.
- Prefer several small, reviewable commits over one large commit.
- Stage explicit paths and review the staged diff before committing. Never use a
  blind `git add -A` or `git add .`.
- Every commit must leave the project buildable and the full quality gate green.
- Do not amend, rebase, force-push, or bypass hooks unless the user explicitly
  asks. Never use `--no-verify` to hide a failure.

## Quality gate

Run before every commit:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --all-targets --all-features
cargo build --all-targets --all-features
```

- Fix failures and rerun the full gate. Do not silence lints with unapproved
  `#[allow(...)]` attributes.
- Run `cargo build --release --all-targets --all-features` before release or
  version-tag commits to match CI.
- When JavaScript helpers are added, also run:

```powershell
node --test src/views/pagination.test.js
```

## Commit messages

Use Conventional Commits with a concise, informative message:

```text
feat(scope): imperative summary
fix(scope): imperative summary
refactor(scope): imperative summary
test(scope): imperative summary
docs(scope): imperative summary
chore(scope): imperative summary
```

- Keep the subject lowercase, imperative, and at most 72 characters.
- Add a body only when it explains context, impact, or a trade-off that the
  subject cannot carry. Keep it short (normally 1-4 lines) and wrap at 72
  characters. Do not restate the diff or list changed files.
- Do not add `Co-authored-by`, `Generated-by`, AI attribution, or similar
  trailers.

## Code standards

- Write clean, efficient, safe Rust. Prefer clarity and explicit behavior over
  clever shortcuts.
- Follow existing modules and naming conventions before introducing new ones.
- Prefer typed structures and explicit validation over ad-hoc JSON handling.
- Keep the RPC boundary, policy decisions, and secret handling centralized.
- Use the typed errors in `error.rs`; avoid `unwrap` and `expect` on fallible
  paths outside tests. Never swallow errors or leave silent fallbacks.
- Never log, serialize, or return credentials.
- Reject malformed input before it reaches Odoo.
- Do not block the async runtime; keep I/O asynchronous and bounded.
- Do not add a dependency unless the standard library or an existing dependency
  cannot do the job cleanly.
- Preserve backward compatibility for existing `config.json` files or provide a
  documented migration path.
- Add focused unit or contract tests for every new behavior and security
  boundary. Use `src/test_support.rs` and mocked Odoo responses; tests must not
  require a live server.
- Keep the UI accessible and consistent with the existing design system.

## Security

- Never commit `.env`, `config.json`, credentials, tokens, or generated secrets.
  `config.example.json` is the only configuration file that belongs in git.
- Keep authorization defaults deny-by-default. Do not weaken policy checks,
  secret redaction, or mutation safeguards to make a test pass.
- Review staged changes for accidental secret or debug-output leakage before
  every commit.

## Definition of done

- The change has one purpose and includes its tests and documentation.
- The full quality gate passes.
- No secrets, credentials, or generated artifacts are staged.
- The commit message follows the format above.
