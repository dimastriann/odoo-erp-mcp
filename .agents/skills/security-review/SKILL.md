---
name: security-review
description: Use before committing changes that touch authentication, authorization, secrets, audit, redaction, Odoo RPC, or request validation. Reviews secret leakage, default-deny policy, query protection, and mutation safeguards.
---

# Security Review

`AGENTS.md` is the canonical rule set. Run this review before committing any
change that touches a trust boundary.

## 1. Secrets and credentials

- `SecretString` must keep redacted `Debug` and `Serialize`.
- Environment-backed credentials stay in `password_env`; resolved values are
  never written back to `config.json`.
- Never log, serialize, return, or put credentials in errors, audit details,
  metrics, or UI responses.
- `redacted_config_value` must keep returning an empty `password` and
  `has_password` instead of the secret.
- Check tests still assert that canary values never appear in responses.

## 2. Authorization

- Keep policy default-deny when `permissions` is enabled.
- Preserve precedence: field and method, operation, model, instance, then
  global.
- Deny must win over allow in the same scope.
- `requires_model`, `requires_client`, capability mapping, and operation
  identity must match the tool being executed.
- Do not add allow rules, legacy-mode shortcuts, or test-only bypasses to make
  a test pass.
- Reject unknown or conflicting policy input before saving it.

## 3. Input and query protection

- Domains are parsed as nested clauses with allowed operators and prefix
  logical operators; no raw Odoo domain strings.
- Enforce `max_domain_depth`, `max_domain_terms`, `max_domain_in_values`,
  `max_requested_fields`, `max_read_ids`, `max_query_limit`, and
  `max_response_records`.
- Validate `order` clauses; reject injection and unsupported tokens.
- Reject malformed input before it reaches Odoo.

## 4. Mutations and audit

- Mutations still require preview, risk classification, approval when needed,
  idempotency, single execution, verification, and audit.
- Never retry a mutation automatically.
- Audit details are redacted and truncated; hash-chain verification must keep
  working.
- Approval and idempotency state transitions must remain guarded.

## 5. Session and transport

- UI sessions use HttpOnly, SameSite=Strict cookies.
- Keep authentication on every admin `/api` route; only `/`, static assets,
  `/health`, `/ready`, `/api/version`, `/api/login`, and `/api/logout` stay
  public.
- Do not disable TLS or HTTPS warnings, and keep `ODOO_MCP_UI_BIND` local-only
  by default.
- Keep response-size limits and timeouts enforced.

## 6. Tests and review

- Add regression tests for every security boundary changed.
- Run the full quality gate from `AGENTS.md`.
- Review the staged diff specifically for secrets, debug output, and weakened
  defaults before committing.
