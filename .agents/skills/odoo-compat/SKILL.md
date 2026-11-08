---
name: odoo-compat
description: Use when changing Odoo version handling, RPC transport, authentication, retries, timeouts, error classification, or response parsing. Ensures Odoo v12-v19+ compatibility without live-server tests.
---

# Odoo Compatibility

`AGENTS.md` is the canonical rule set. Keep one client boundary in
`src/odoo/`; tools and lifecycle code must not depend on transport details.

## 1. Transport selection

- `RpcTransport` supports `auto`, `jsonrpc`, and `json2`.
- `auto` probes `common.version`; use JSON-2 for Odoo 19+ and fall back to
  JSON-RPC when JSON-2 rejects the credential.
- Keep the configured and current transport distinction and the atomic
  transport switch in `src/odoo/client.rs`.
- JSON-2 uses bearer API keys and named arguments; JSON-RPC uses the classic
  envelope. Preserve request-body and response-normalization behavior.

## 2. Version handling

- Parse `server_version_info` first, then `server_version` as a fallback.
- Treat unknown versions conservatively; do not assume JSON-2 support.
- Keep version-probing failures non-fatal for auto mode.
- Update the README support matrix only when behavior actually changes.

## 3. Errors and retries

- Classify transport, timeout, protocol, authentication, validation, and
  access errors through `AppError`; never string-match raw Odoo text.
- Retry only read-only operations, only transient errors, with bounded
  exponential backoff and jitter.
- Never retry mutations or authentication failures.
- Preserve response-size limits and timeouts.
- Record retry metrics and events without leaking payloads or credentials.

## 4. Odoo semantics

- Respect JSON-RPC versus JSON-2 differences for create and read_group results.
- Keep method names and argument shapes version-compatible.
- Do not add Odoo-version-specific behavior inside tool handlers; keep it in
  the client or a dedicated compatibility module.
- Preserve `context` validation and per-instance query limits.

## 5. Tests

Use `src/test_support.rs` mocked servers; no live Odoo instance.

- transport auto-selection for Odoo 18 and 19+;
- JSON-2 bearer headers and named arguments;
- JSON-2 result normalization;
- fallback from JSON-2 to JSON-RPC on rejected credentials;
- timeout, protocol, authentication, and malformed-envelope categories;
- retry success, retry exhaustion, and no-retry for mutations and auth;
- response-size and timeout limits.

## 6. Review

- Run the full quality gate from `AGENTS.md`.
- Check that the change does not weaken TLS verification, credential handling,
  or error redaction.
