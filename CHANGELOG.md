# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] — 2026-09-06

### Added

- Added consistent pagination to search and search-read results, including
  optional total counts through `include_total`.
- Added configurable global and per-instance limits for query size, requested
  fields, read IDs, domain depth, domain terms, membership values, and response
  record counts.
- Added typed Odoo domain parsing with explicit logical operators and a safe
  comparison-operator allowlist.
- Added security regression coverage proving malformed and excessive domains
  are rejected before any Odoo RPC call.

### Changed

- Applied a safe default limit to search operations when callers omit `limit`.
- Standardized paginated responses with `items`, `offset`, `limit`, and
  `has_more` metadata.
- Enforced logical operator arity and bounded `in` and `not in` operands.
- Exposed query-protection settings in the administration UI while preserving
  valid custom settings during configuration updates.

### Fixed

- Prevented malformed flat domains and unsupported operators from reaching
  Odoo and returning server-side tracebacks.
- Stabilized retry timeout fixtures under local scheduler contention.

### Compatibility notes

- Existing `0.4.0` configuration files remain compatible; query-protection
  settings use secure defaults when absent.
- No MCP tools were removed or renamed.
- Search and search-read success payloads now use the documented pagination
  envelope instead of returning an unwrapped array.

## [0.4.0] — 2026-09-03

### Added

- Added structured MCP error responses with stable machine-readable codes and
  retryability metadata.
- Added configurable Odoo connection and request timeouts.
- Added a configurable response-body size limit with bounded streaming.
- Added conservative automatic retries for proven-safe read-only operations
  using capped exponential backoff with jitter.
- Added sanitized retry events that can feed logging and metrics observers.
- Added regression coverage for authentication, HTTP failures, malformed
  responses, timeouts, retry exhaustion, eventual recovery, and mutation safety.

### Changed

- Classified configuration, authentication, authorization, Odoo business,
  transport, timeout, protocol, and internal failures with typed errors.
- Assigned a unique identifier to every JSON-RPC request attempt.
- Validated HTTP status and JSON-RPC response envelopes before returning data.
- Organized MCP transport, Odoo integration, and tool implementation into
  dedicated `mcp`, `odoo`, and `tools` modules.

### Fixed

- Prevented Odoo traceback, context, endpoint, and credential details from
  leaking through MCP error responses.
- Preserved transport and protocol categories instead of reporting every client
  construction failure as an authentication error.
- Preserved custom timeout settings when changing the global mode in the Web UI.
- Ensured mutation and authentication requests are never retried automatically.

### Compatibility notes

- Existing `0.3.1` configuration files remain compatible; new RPC settings use
  backward-compatible defaults when absent.
- No MCP tools were removed or renamed.
- Error responses now include structured error metadata in addition to readable
  MCP error content.

## [0.3.1] — 2026-09-01

This is the first tagged release after `v0.2.0`. The internal `0.3.0` version
was not published as a Git tag, so this entry also includes its unreleased
changes.

### Added

- Added a protected Web administration UI for managing Odoo instances,
  permissions, and prompts.
- Added CI checks for formatting, Clippy, tests, and debug/release builds.
- Added reusable multi-instance and mock Odoo JSON-RPC test infrastructure.
- Added contract tests for all existing MCP tools.
- Added an implementation roadmap covering production safety through optional
  agent capabilities.
- Declared Rust 1.85 as the minimum supported toolchain.

### Changed

- Split MCP tool declarations, arguments, execution, and results into focused
  modules.
- Replaced free-form tool dispatch with typed tool names.
- Added typed deserialization for read and write tool arguments.
- Consolidated successful tool-result serialization.
- Standardized repository formatting and line-ending rules.

### Fixed

- MCP initialization now reports the Cargo package version instead of a stale
  hard-coded version.
- Odoo domain schemas now describe nested filter clauses explicitly.
- Flat Odoo filters such as `["name", "=", "S00027"]` are rejected locally
  with a corrective example before reaching Odoo.
- Existing strict-Clippy violations in permission evaluation were resolved.

### Compatibility notes

- Existing `0.3.0` configuration files remain compatible with `0.3.1`.
- Tool argument validation is stricter: record IDs must be integers, write
  values must be JSON objects, and domain filters must use nested clauses.
- No MCP tools were removed or renamed.

[0.5.0]: https://github.com/dimastriann/odoo-erp-mcp/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/dimastriann/odoo-erp-mcp/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/dimastriann/odoo-erp-mcp/compare/v0.2.0...v0.3.1
