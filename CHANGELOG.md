# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.3.1]: https://github.com/dimastriann/odoo-erp-mcp/compare/v0.2.0...v0.3.1
