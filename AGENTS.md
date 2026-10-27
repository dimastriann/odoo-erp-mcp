# Agent Workflow

This repository is an MCP gateway for Odoo. Changes must preserve the typed
error contract, authorization policy, secret redaction, mutation lifecycle, and
query-protection invariants.

## Atomic commits

- One logical purpose per commit. Do not mix refactoring, features, UI work, or
  documentation that are not required by that purpose.
- Tests for a behavior belong in the same commit as the behavior.
- Do not create WIP, empty, test-later, or formatting-only commits.
- Prefer several small, reviewable commits over one large commit.
- Every commit must leave the project buildable and the full quality gate green.

## Quality gate

Run before every commit:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --all-targets --all-features
cargo build --all-targets --all-features
```

When JavaScript helpers are added, also run:

```powershell
node --test src/views/pagination.test.js
```

## Commit messages

Follow the existing history style:

```text
feat(scope):short-imperative-description
fix(scope):short-imperative-description
test(scope):short-imperative-description
docs(scope):short-imperative-description
chore(release):short-imperative-description
```

Keep the subject concise. Use the body only when the reason or trade-off is not
obvious.

## Code standards

- Follow existing modules and naming conventions before introducing new ones.
- Prefer typed structures and explicit validation over ad-hoc JSON handling.
- Keep the RPC boundary, policy decisions, and secret handling centralized.
- Never log, serialize, or return credentials.
- Reject malformed input before it reaches Odoo.
- Preserve backward compatibility for existing `config.json` files or provide a
  documented migration path.
- Add focused unit or contract tests for every new behavior and security
  boundary.
- Keep the UI accessible and consistent with the existing design system.
