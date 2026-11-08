---
name: mcp-tool-extension
description: Use when adding, changing, or removing an MCP tool in this repository. Covers catalog schemas, argument validation, executor dispatch, typed results, capability policy, query limits, tests, and documentation.
---

# MCP Tool Extension

`AGENTS.md` is the canonical rule set. Follow this checklist for every tool
change.

## 1. Define the contract

- Choose a stable `odoo-*` name and the matching `ToolName` variant in
  `src/tools/catalog.rs`.
- Add the name to `ToolName::ALL`, `as_str`, `TryFrom<&str>`, and
  `capability`.
- Decide `requires_model` and `requires_client`.
- Add the JSON `inputSchema` in `tool_definitions` with descriptions, defaults,
  required fields, and bounded arrays/strings.

## 2. Validate before RPC

- Add a typed argument struct in `src/tools/arguments.rs` following the
  existing naming and serde conventions.
- Reject missing, malformed, unknown, and oversized input before any Odoo call.
- Reuse `validate_domain_security`, `validate_field_names`,
  `validate_field_count`, `validate_order`, `resolve_limit`, and
  `validate_read_id_count` where applicable.
- Keep pagination `limit`/`offset` bounded by configured `QueryLimits`.

## 3. Implement execution

- Add the dispatch arm in `src/tools/executor.rs::execute_tool`.
- Use the `OdooClient`; never call Odoo directly from the MCP layer.
- Return `ToolExecutionResult` and map errors through `AppError`; do not
  string-match Odoo errors.
- For reads, use the pagination helpers and response-size protection.
- For mutations, do not add a second lifecycle path: route through
  `Operation`/`OperationExecutor` and follow the `mutation-lifecycle` skill.

## 4. Policy and safety

- Map the tool to the narrowest `Capability` in `catalog.rs`.
- Confirm instance, model, operation, field, and method policy scopes still
  apply.
- Default-deny must remain intact; never add an allow rule to make a test pass.
- Never log, serialize, or return credentials or raw Odoo payload secrets.

## 5. Tests

Add focused tests that do not require a live Odoo server:

- Schema: the tool definition exists, required fields are enforced, bounds are
  correct, and the `ToolName` round-trip works.
- Arguments: missing, malformed, unknown, and oversized input is rejected.
- Policy: allow and deny paths, including operation and model overrides.
- Executor: mock-server success, Odoo error mapping, pagination, and limits.
- Result: success and failure shapes; approval-required is not a transport
  error.

Use `src/test_support.rs` and the existing mocked responses.

## 6. Documentation and review

- Update the tool list and examples in `README.md` when behavior changes.
- Update `ROADMAP.md` only when the item is defined there.
- Check version references only if the tool changes the advertised server
  capability or version.
- Run the full quality gate from `AGENTS.md` before committing.
