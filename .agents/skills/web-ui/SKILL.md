---
name: web-ui
description: Use when changing the local Odoo configuration UI, Axum routes, static assets, pagination helpers, or admin approval inbox. Covers accessibility, credential safety, API shape, and the JavaScript test gate.
---

# Web UI

`AGENTS.md` is the canonical rule set. The UI is the admin surface for
configuration, policy, approvals, and audit.

## 1. Server routes

- Add or change routes in `src/views/web_ui.rs`.
- Put authenticated `/api` routes in the `protected` router; public routes stay
  limited to `/`, static assets, `/health`, `/ready`, `/api/version`,
  `/api/login`, and `/api/logout`.
- Keep `require_auth` on every new admin API route.
- Use typed request structs and `serde` validation; reject malformed IDs and
  payloads before mutating configuration.
- Return stable JSON shapes and appropriate status codes.

## 2. Credential safety

- `redacted_config_value` must keep secrets out of `/api/config`.
- Never return `password` or resolved environment values; expose only
  `has_password` and `password_env`.
- Preserve the behavior where a blank password keeps the existing secret only
  for the matching credential source.
- Do not log credentials, session tokens, or request bodies containing
  secrets.
- Keep session cookies `HttpOnly` and `SameSite=Strict`.

## 3. UI behavior

- `src/views/index.html` and `src/views/app.js` use Vue and the existing design
  tokens in `src/views/styles.css`; follow the current class and component
  patterns.
- Keep forms labeled and keyboard reachable; preserve `aria-label`, `label`,
  and focus behavior.
- Validate policies before saving: conflicting allow and deny lists, malformed
  JSON, and empty default-deny policies require explicit confirmation.
- Show default-deny warnings and never imply that unmatched capabilities are
  allowed.
- Keep pagination in `pagination.js`; do not duplicate page math in `app.js`.

## 4. Static assets

- Serve assets through the existing handlers (`app_js`, `pagination_js`,
  `styles_css`); do not add a catch-all file server.
- Keep the UI local-first; do not add remote scripts, fonts, or telemetry.
- Keep dark/light theme behavior and localStorage keys stable.

## 5. Tests

- Run `node --test src/views/pagination.test.js` when JavaScript or pagination
  changes.
- Add Rust tests in `src/views/web_ui.rs` for:
  - credential redaction and environment-secret absence;
  - switching credential sources;
  - stable config, approval, and audit response shapes;
  - malformed approval IDs;
  - health and readiness responses.
- Run the full quality gate from `AGENTS.md`.

## 6. Review

- Check the rendered UI at `http://127.0.0.1:3333` when practical.
- Verify no secret appears in page data, network responses, or console output.
- Keep `ODOO_MCP_UI_BIND` default local-only.
