# Odoo ERP MCP — Implementation Roadmap

This document is the canonical plan for evolving `odoo-erp-mcp` from an MCP
CRUD server into a secure, reliable, semantics-aware Odoo gateway for AI agents.
It is organized by technical dependency, risk reduction, and atomic delivery.

## Status and usage

- `[ ]` Not started
- `[~]` In progress
- `[x]` Completed and verified
- `[-]` Deferred or no longer required

When an item is completed:

1. Meet its acceptance criteria.
2. Run all relevant quality gates.
3. Create an atomic commit.
4. Mark it `[x]` and optionally record the commit SHA.
5. Record important architectural decisions in the Decision Log.

## Delivery principles

Every commit must have one clear purpose, be independently reviewable, include
tests for new behavior, and leave the project buildable. Do not combine unrelated
refactoring, features, UI work, or documentation. Preserve backward compatibility
or provide an explicit migration path.

The target of at least five commits per active day is a delivery cadence, not a
reason to create empty, WIP, incomplete, or artificially fragmented commits.

### Quality gate

Run before every commit:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --all-targets --all-features
```

Add integration, migration, security-regression, UI, and dependency checks when
relevant to the change.

## Priority order

| Priority | Focus |
|---|---|
| P0-A | Baseline engineering and testability |
| P0-B | Structured errors, RPC reliability, and safe read-only operation |
| P0-C | Identity, secret safety, and authorization policy |
| P0-D | Safe mutation lifecycle, audit, and observability |
| P1-A | Odoo semantics, metadata, and workflows |
| P1-B | Business tools and analytics |
| P1-C | Remote MCP and enterprise readiness |
| P2 | Natural-language queries, agents, and automation |
| P3 | Optional optimization and platform expansion |

---

# Stage 0 — Baseline Engineering and Testability

Goal: Establish a modular, testable foundation before adding production features.

## 0.1 Repository baseline

- [x] S0-01 — Align the Cargo, MCP `serverInfo`, UI, and documentation versions.
- [x] S0-02 — Use `CARGO_PKG_VERSION` as the single runtime version source.
- [x] S0-03 — Document the minimum supported Rust version.
- [x] S0-04 — Add `.editorconfig` if it is not already present.
- [x] S0-05 — Add CI formatting checks.
- [x] S0-06 — Add CI Clippy checks with warnings treated as errors.
- [x] S0-07 — Add CI unit and integration test execution.
- [x] S0-08 — Add debug and release build verification.

## 0.2 Test infrastructure

- [x] S0-09 — Create a reusable multi-instance configuration fixture.
- [x] S0-10 — Create a mock Odoo JSON-RPC server.
- [x] S0-11 — Add an RPC request recorder for integration tests.
- [x] S0-12 — Add successful Odoo response fixtures.
- [x] S0-13 — Add Odoo validation and access-error fixtures.
- [x] S0-14 — Add timeout and connection-failure fixtures.
- [x] S0-15 — Add contract tests for all existing MCP tools.

## 0.3 MCP modularization

- [x] S0-16 — Separate tool declarations from the MCP request dispatcher.
- [x] S0-17 — Create a typed `ToolName`.
- [x] S0-18 — Create typed argument structures for read tools.
- [x] S0-19 — Create typed argument structures for write tools.
- [x] S0-20 — Create a typed tool-execution result.
- [x] S0-21 — Separate the tool registry from the tool executor.
- [x] S0-22 — Remove duplicate result-serialization logic.
- [x] S0-23 — Add registry and argument-validation tests.

## Stage 0 Definition of Done

- [x] Versions are consistent across the application.
- [x] All existing tools have contract tests.
- [x] The dispatcher no longer contains all business logic.
- [x] CI verifies formatting, linting, tests, and builds.

---

# Stage 1 — Structured Errors and RPC Reliability

Goal: Make MCP-to-Odoo communication understandable, classifiable, reliable, and testable.

## 1.1 Typed error model

- [x] S1-01 — Create the main internal error enum.
- [x] S1-02 — Add an input-validation error category.
- [x] S1-03 — Add a configuration-error category.
- [ ] S1-04 — Add authentication and authorization error categories.
- [ ] S1-05 — Add Odoo validation and access-error categories.
- [ ] S1-06 — Add transport, timeout, and protocol-error categories.
- [ ] S1-07 — Add stable machine-readable error codes.
- [ ] S1-08 — Add retryability classification.
- [ ] S1-09 — Create a structured MCP error response.
- [ ] S1-10 — Migrate one read tool as the reference implementation.
- [ ] S1-11 — Migrate all remaining read tools.
- [ ] S1-12 — Migrate all write tools.
- [ ] S1-13 — Sanitize errors to prevent credential and secret disclosure.
- [ ] S1-14 — Add regression tests for every error category.

## 1.2 RPC client hardening

- [ ] S1-15 — Use a unique JSON-RPC request ID.
- [ ] S1-16 — Validate HTTP status before parsing the response.
- [ ] S1-17 — Validate the JSON-RPC result/error envelope.
- [ ] S1-18 — Classify malformed JSON-RPC responses.
- [ ] S1-19 — Add a configurable connection timeout.
- [ ] S1-20 — Add a configurable request timeout.
- [ ] S1-21 — Add response-body size protection.
- [ ] S1-22 — Add a typed authentication failure.
- [ ] S1-23 — Add timeout, malformed-response, and HTTP-failure tests.

## 1.3 Safe retry foundation

- [ ] S1-24 — Create retry classifications by operation class.
- [ ] S1-25 — Add exponential backoff with jitter.
- [ ] S1-26 — Retry only proven-safe read-only operations.
- [ ] S1-27 — Ensure write operations are never retried automatically.
- [ ] S1-28 — Add retry event and metrics hooks.
- [ ] S1-29 — Add retry-exhaustion and eventual-success tests.

## Stage 1 Definition of Done

- [ ] RPC and tool errors use a structured schema.
- [ ] Connection and request timeouts are configurable.
- [ ] Automatic retries apply only to operations proven safe.
- [ ] Secrets never appear in error responses.

---

# Stage 2 — Query Protection and Safe Read-Only Core

Goal: Provide a production-appropriate read-only deployment mode.

## 2.1 Pagination

- [ ] S2-01 — Add `offset` and `limit` to `odoo-search`.
- [ ] S2-02 — Add `offset` and `limit` to `odoo-search-read`.
- [ ] S2-03 — Add a default query limit.
- [ ] S2-04 — Add a configurable maximum query limit.
- [ ] S2-05 — Reject negative offsets and invalid limits.
- [ ] S2-06 — Add pagination metadata.
- [ ] S2-07 — Add `has_more` without always running an expensive count.
- [ ] S2-08 — Add an optional total count.
- [ ] S2-09 — Add pagination contract tests.

## 2.2 Field, record, and payload protection

- [ ] S2-10 — Add a maximum requested-field count.
- [ ] S2-11 — Reject duplicate and invalid field names.
- [ ] S2-12 — Define behavior when the field list is empty.
- [ ] S2-13 — Add a maximum ID count for `odoo-read`.
- [ ] S2-14 — Add a maximum domain depth.
- [ ] S2-15 — Add a maximum domain-term count.
- [ ] S2-16 — Add a maximum response-record count.
- [ ] S2-17 — Add query-protection configuration validation.
- [ ] S2-18 — Document defaults and per-instance overrides.

## 2.3 Domain validation

- [ ] S2-19 — Create a typed Odoo domain validator.
- [ ] S2-20 — Validate allowed domain operators.
- [ ] S2-21 — Validate logical operators and arity.
- [ ] S2-22 — Limit values supplied to `in` and `not in`.
- [ ] S2-23 — Reject malformed domains before calling Odoo.
- [ ] S2-24 — Add domain security-regression tests.

## Stage 2 Definition of Done

- [ ] No search operation is unlimited.
- [ ] Excessive queries are rejected before reaching Odoo.
- [ ] Pagination responses are consistent.
- [ ] Read-only mode has production-safe limits.

---

# Stage 3 — Identity, Secrets, and Authorization

Goal: Identify every actor and agent, protect credentials, and enforce granular policies.

## 3.1 Request identity

- [ ] S3-01 — Create a typed `RequestContext`.
- [ ] S3-02 — Add a correlation/request ID.
- [ ] S3-03 — Add client and agent identity.
- [ ] S3-04 — Separate actor identity from agent identity.
- [ ] S3-05 — Define trusted and untrusted identity sources.
- [ ] S3-06 — Propagate request context through every executor.
- [ ] S3-07 — Add identity-propagation tests.

## 3.2 Secret handling

- [ ] S3-08 — Define a `SecretProvider` abstraction.
- [ ] S3-09 — Use a redacted secret type for Odoo credentials.
- [ ] S3-10 — Prevent debug output and serialization from disclosing secrets.
- [ ] S3-11 — Add an environment-backed secret provider.
- [ ] S3-12 — Retain legacy inline secrets with a migration warning.
- [ ] S3-13 — Add a local configuration file-permission warning.
- [ ] S3-14 — Document secret configuration and migration.
- [ ] S3-15 — Add secret-redaction regression tests.

## 3.3 Capability policy

- [ ] S3-16 — Create typed read/create/update/delete capabilities.
- [ ] S3-17 — Add workflow, financial, and admin capabilities.
- [ ] S3-18 — Create explicit `allow` and `deny` policy decisions.
- [ ] S3-19 — Retain compatibility with `crud` and `read_only` modes.
- [ ] S3-20 — Add per-instance permissions.
- [ ] S3-21 — Add per-model permissions.
- [ ] S3-22 — Add per-operation permissions.
- [ ] S3-23 — Add field-level allow and deny rules.
- [ ] S3-24 — Add method and workflow permissions.
- [ ] S3-25 — Define policy precedence.
- [ ] S3-26 — Apply default-deny behavior to the new policy system.
- [ ] S3-27 — Explain denied policy decisions in responses.
- [ ] S3-28 — Add compatibility and policy-matrix tests.
- [ ] S3-29 — Document configuration migration.

## Stage 3 Definition of Done

- [ ] Every operation carries actor, agent, request, and instance identity.
- [ ] Secrets never appear in responses, logs, or debug output.
- [ ] Authorization no longer depends only on tool names.
- [ ] Legacy configuration has a documented migration path.

---

# Stage 4 — Safe Mutation Lifecycle

Goal: Route writes through authorization, risk assessment, preview, approval, idempotency, execution, verification, and audit.

## 4.1 Operation envelope and executor

- [ ] S4-01 — Define a typed `Operation`.
- [ ] S4-02 — Add a unique operation ID.
- [ ] S4-03 — Add read, write, workflow, financial, and admin operation classes.
- [ ] S4-04 — Add a normalized operation payload.
- [ ] S4-05 — Add an immutable payload hash.
- [ ] S4-06 — Create a central `OperationExecutor`.
- [ ] S4-07 — Migrate create operations to the executor.
- [ ] S4-08 — Migrate update operations to the executor.
- [ ] S4-09 — Migrate delete operations to the executor.
- [ ] S4-10 — Migrate copy operations to the executor.
- [ ] S4-11 — Add operation-lifecycle tests.

## 4.2 Risk assessment

- [ ] S4-12 — Define low, medium, high, and critical risk levels.
- [ ] S4-13 — Classify risk by operation type.
- [ ] S4-14 — Add model-specific risk overrides.
- [ ] S4-15 — Add field-specific risk overrides.
- [ ] S4-16 — Add bulk-operation thresholds.
- [ ] S4-17 — Classify financial workflows as high or critical risk.
- [ ] S4-18 — Return the reason for each risk classification.
- [ ] S4-19 — Add risk-matrix tests.

## 4.3 Preview and validation

- [ ] S4-20 — Define the preview-response schema.
- [ ] S4-21 — Add create preview without writing to Odoo.
- [ ] S4-22 — Add update preview with current and proposed values.
- [ ] S4-23 — Add delete preview with affected records.
- [ ] S4-24 — Add bulk-operation summary preview.
- [ ] S4-25 — Validate write payloads against model metadata.
- [ ] S4-26 — Mark computed and default values as estimates.
- [ ] S4-27 — Document that preview is not a transactional dry run.
- [ ] S4-28 — Add preview and validation tests.

## 4.4 Durable approval protocol

- [ ] S4-29 — Define the approval-request schema.
- [ ] S4-30 — Create an approval-storage abstraction.
- [ ] S4-31 — Add an initial durable local implementation.
- [ ] S4-32 — Bind approval to the operation payload hash.
- [ ] S4-33 — Bind approval to the actor and instance.
- [ ] S4-34 — Add approval expiration.
- [ ] S4-35 — Add approved and rejected states.
- [ ] S4-36 — Enforce single-use execution.
- [ ] S4-37 — Reject payload modifications after approval.
- [ ] S4-38 — Add approval-timeout handling.
- [ ] S4-39 — Add concurrency and replay tests.
- [ ] S4-40 — Integrate an approval inbox into the admin UI.

## 4.5 Idempotency

- [ ] S4-41 — Define the idempotency-record schema.
- [ ] S4-42 — Create an idempotency-storage abstraction.
- [ ] S4-43 — Accept a client-supplied idempotency key.
- [ ] S4-44 — Scope keys by actor and instance.
- [ ] S4-45 — Bind keys to payload hashes.
- [ ] S4-46 — Add pending, succeeded, failed, and unknown states.
- [ ] S4-47 — Return stored results for completed requests.
- [ ] S4-48 — Reject key reuse with a different payload.
- [ ] S4-49 — Add configurable TTL.
- [ ] S4-50 — Add expired-record cleanup.
- [ ] S4-51 — Treat post-send timeouts as an unknown state.
- [ ] S4-52 — Add crash, replay, and concurrency tests.

## 4.6 Post-action verification

- [ ] S4-53 — Define the verification-result schema.
- [ ] S4-54 — Verify created records.
- [ ] S4-55 — Verify updated fields.
- [ ] S4-56 — Verify deleted-record absence.
- [ ] S4-57 — Detect partial verification.
- [ ] S4-58 — Distinguish execution success from verification success.
- [ ] S4-59 — Add mismatch and unknown-state tests.

## Stage 4 Definition of Done

- [ ] High-risk mutations cannot run without valid human approval.
- [ ] Approved payloads cannot be modified before execution.
- [ ] Duplicate requests do not create duplicate transactions.
- [ ] Write results are verified and expose an explicit final state.
- [ ] All lifecycle paths have automated tests.

---

# Stage 5 — Audit and Observability

Goal: Make operations traceable without exposing secrets or unrestricted sensitive data.

## 5.1 Audit trail

- [ ] S5-01 — Define a versioned audit-event schema.
- [ ] S5-02 — Record authorization decisions.
- [ ] S5-03 — Record preview generation.
- [ ] S5-04 — Record approvals and rejections.
- [ ] S5-05 — Record execution attempts.
- [ ] S5-06 — Record verification results.
- [ ] S5-07 — Record failed, denied, and unknown operations.
- [ ] S5-08 — Add a field-redaction policy.
- [ ] S5-09 — Add an audit-payload size limit.
- [ ] S5-10 — Create an append-only audit-storage abstraction.
- [ ] S5-11 — Add a durable local audit backend.
- [ ] S5-12 — Add retention configuration.
- [ ] S5-13 — Add tamper-evident hash chaining.
- [ ] S5-14 — Add authorized audit query and export.
- [ ] S5-15 — Add sensitive-data regression tests.

## 5.2 Runtime observability

- [ ] S5-16 — Add structured logging.
- [ ] S5-17 — Include correlation IDs in all operation logs.
- [ ] S5-18 — Record operation duration.
- [ ] S5-19 — Record Odoo RPC latency.
- [ ] S5-20 — Add request and error counters.
- [ ] S5-21 — Add retry counters.
- [ ] S5-22 — Add tracing spans.
- [ ] S5-23 — Add log and trace redaction.
- [ ] S5-24 — Add a health endpoint.
- [ ] S5-25 — Add a readiness endpoint.
- [ ] S5-26 — Add a metrics endpoint for the remote runtime.

## Stage 5 Definition of Done

- [ ] All write-lifecycle events are traceable.
- [ ] Audit storage excludes secrets and unrestricted sensitive values.
- [ ] Requests can be traced from MCP through Odoo RPC.
- [ ] Health, readiness, and runtime metrics are available where applicable.

---

# Stage 6 — Odoo Semantic and Workflow Layer

Goal: Let agents discover Odoo semantics and invoke curated business workflows safely.

## 6.1 Semantic metadata

- [ ] S6-01 — Create a typed model schema.
- [ ] S6-02 — Add required, read-only, and computed-field metadata.
- [ ] S6-03 — Add selection and enum metadata.
- [ ] S6-04 — Add relation metadata.
- [ ] S6-05 — Add model-access metadata.
- [ ] S6-06 — Implement `odoo-get-model-schema`.
- [ ] S6-07 — Implement `odoo-get-relations`.
- [ ] S6-08 — Implement `odoo-get-enum-values`.
- [ ] S6-09 — Implement `odoo-get-required-fields`.
- [ ] S6-10 — Add metadata output-size protection.

## 6.2 Metadata cache

- [ ] S6-11 — Create a metadata-cache abstraction.
- [ ] S6-12 — Scope cache entries per instance.
- [ ] S6-13 — Add configurable TTL.
- [ ] S6-14 — Deduplicate concurrent cache misses.
- [ ] S6-15 — Add a manual refresh operation.
- [ ] S6-16 — Invalidate cache entries when instance configuration changes.
- [ ] S6-17 — Add cache metrics.
- [ ] S6-18 — Add cache-correctness tests.

## 6.3 Relationship graph

- [ ] S6-19 — Build an in-memory adjacency graph from metadata.
- [ ] S6-20 — Add Many2one traversal.
- [ ] S6-21 — Add One2many traversal.
- [ ] S6-22 — Add Many2many traversal.
- [ ] S6-23 — Add relationship-path search with a depth limit.
- [ ] S6-24 — Add a relationship-explanation tool.
- [ ] S6-25 — Add cycle and large-graph protection.

## 6.4 Curated workflow methods

- [ ] S6-26 — Define a workflow-method registry.
- [ ] S6-27 — Apply a strict method allowlist.
- [ ] S6-28 — Add method-specific argument schemas.
- [ ] S6-29 — Integrate workflows with capability policy.
- [ ] S6-30 — Integrate workflows with risk assessment and approval.
- [ ] S6-31 — Implement an internal generic method caller.
- [ ] S6-32 — Expose only curated methods.
- [ ] S6-33 — Add sales-order confirmation.
- [ ] S6-34 — Add invoice posting.
- [ ] S6-35 — Add stock-picking validation.
- [ ] S6-36 — Add explicit cancellation workflows.
- [ ] S6-37 — Add supported-Odoo-version compatibility tests.

## Stage 6 Definition of Done

- [ ] Agents can discover schemas and relationships without guessing.
- [ ] Metadata queries use a per-instance cache.
- [ ] No arbitrary public method execution is exposed.
- [ ] Workflows pass through policy, approval, verification, and audit.

---

# Stage 7 — Business Tools and Analytics

Goal: Expose stable, protected tools expressed in business language.

## 7.1 Aggregation foundation

- [ ] S7-01 — Create typed metrics.
- [ ] S7-02 — Create typed group-by expressions.
- [ ] S7-03 — Add date-interval validation.
- [ ] S7-04 — Add ranking and maximum-group protection.
- [ ] S7-05 — Add currency and company context.
- [ ] S7-06 — Create a generic aggregation response.
- [ ] S7-07 — Add aggregation query protection.

## 7.2 Sales tools

- [ ] S7-08 — Implement sales summary.
- [ ] S7-09 — Implement sales revenue.
- [ ] S7-10 — Implement pending sales orders.
- [ ] S7-11 — Implement top customers.
- [ ] S7-12 — Implement inactive customers.

## 7.3 Inventory tools

- [ ] S7-13 — Implement current stock.
- [ ] S7-14 — Implement a low-stock report.
- [ ] S7-15 — Implement a reserved-stock report.
- [ ] S7-16 — Implement a pending-delivery report.
- [ ] S7-17 — Implement an inventory summary.

## 7.4 CRM tools

- [ ] S7-18 — Implement CRM pipeline reporting.
- [ ] S7-19 — Implement lead queries.
- [ ] S7-20 — Implement conversion-rate analysis.
- [ ] S7-21 — Implement customer-activity summaries.

## 7.5 Accounting tools

- [ ] S7-22 — Implement a receivable report.
- [ ] S7-23 — Implement a payable report.
- [ ] S7-24 — Implement an overdue-invoice report.
- [ ] S7-25 — Implement a cash-flow summary.

## 7.6 Purchase tools

- [ ] S7-26 — Implement pending purchase orders.
- [ ] S7-27 — Implement a supplier summary.
- [ ] S7-28 — Implement supplier price-change analysis.

## Stage 7 Definition of Done

- [ ] Business tools use Odoo business semantics rather than guessed fields.
- [ ] Every tool has query protection and permission checks.
- [ ] Currency, company, date range, and timezone are explicit.
- [ ] Every tool has contract and compatibility tests.

---

# Stage 8 — Remote MCP and Enterprise Readiness

Goal: Support authenticated remote access, organizational isolation, and production deployment.

## 8.1 Remote transport and authentication

- [ ] S8-01 — Separate the transport-independent MCP core.
- [ ] S8-02 — Implement the selected remote MCP transport.
- [ ] S8-03 — Enforce HTTPS.
- [ ] S8-04 — Add API-key authentication as the baseline.
- [ ] S8-05 — Add token hashing and rotation.
- [ ] S8-06 — Add JWT validation.
- [ ] S8-07 — Add OAuth 2.1 for the selected deployment model.
- [ ] S8-08 — Add per-principal rate limiting.
- [ ] S8-09 — Add request-body limits.
- [ ] S8-10 — Add session and connection management.

## 8.2 RBAC and tenant isolation

- [ ] S8-11 — Add role management.
- [ ] S8-12 — Add user-to-role mapping.
- [ ] S8-13 — Add organization and tenant identity.
- [ ] S8-14 — Add user-to-instance mapping.
- [ ] S8-15 — Scope persistent storage per tenant.
- [ ] S8-16 — Scope secrets per tenant.
- [ ] S8-17 — Add cross-tenant isolation tests.

## 8.3 Production deployment

- [ ] S8-18 — Add a production Dockerfile.
- [ ] S8-19 — Add a Docker Compose deployment.
- [ ] S8-20 — Add an environment-configuration reference.
- [ ] S8-21 — Document reverse-proxy and TLS setup.
- [ ] S8-22 — Add Kubernetes manifests when required.

## Stage 8 Definition of Done

- [ ] Remote access requires a trusted identity.
- [ ] Rate, request, and connection limits are enforced.
- [ ] Tenants cannot access another tenant's configuration, secrets, or data.
- [ ] Production deployment and recovery procedures are documented.

---

# Stage 9 — Strategic and Optional Capabilities

Goal: Add higher-level intelligence only after the safety, semantic, and enterprise foundations are stable.

## 9.1 Natural-language query

- [ ] S9-01 — Define a typed intent schema.
- [ ] S9-02 — Implement a model resolver.
- [ ] S9-03 — Implement a field resolver.
- [ ] S9-04 — Implement a relationship resolver.
- [ ] S9-05 — Implement a constrained domain generator.
- [ ] S9-06 — Add domain explanations.
- [ ] S9-07 — Add query-cost assessment.
- [ ] S9-08 — Add a human-readable execution plan.
- [ ] S9-09 — Restrict initial execution to read-only operations.
- [ ] S9-10 — Create an evaluation dataset and accuracy tests.

## 9.2 Domain agents

- [ ] S9-11 — Implement a Sales Agent using curated business tools.
- [ ] S9-12 — Implement an Inventory Agent.
- [ ] S9-13 — Implement a Finance Agent.
- [ ] S9-14 — Implement a CRM Agent.
- [ ] S9-15 — Implement a Purchase Agent.

## 9.3 Scheduling and autonomous workflows

- [ ] S9-16 — Create a durable scheduler abstraction.
- [ ] S9-17 — Add recurring task definitions.
- [ ] S9-18 — Add an event-trigger abstraction.
- [ ] S9-19 — Add a notification-channel abstraction.
- [ ] S9-20 — Add approval timeout and escalation.
- [ ] S9-21 — Add workflow checkpointing.
- [ ] S9-22 — Add recovery and compensation handling.
- [ ] S9-23 — Implement a low-stock recommendation workflow.
- [ ] S9-24 — Implement an overdue-invoice reminder workflow.
- [ ] S9-25 — Complete a dedicated security review before autonomous writes.

## 9.4 Optional platform expansion

- [ ] S9-26 — Evaluate a graph database based on profiling evidence.
- [ ] S9-27 — Add external audit backends.
- [ ] S9-28 — Add external secret managers.
- [ ] S9-29 — Add advanced operational dashboards.
- [ ] S9-30 — Evaluate a Kubernetes operator.
- [ ] S9-31 — Add AI-generated report layouts.
- [ ] S9-32 — Evaluate predictive analytics.
- [ ] S9-33 — Add multi-level approvals.
- [ ] S9-34 — Add external approval connectors.
- [ ] S9-35 — Evaluate a plugin ecosystem.

## Stage 9 Definition of Done

- [ ] Natural-language execution remains constrained and explainable.
- [ ] Domain agents use policy-aware curated tools rather than arbitrary RPC.
- [ ] Autonomous workflows are durable, recoverable, and approval-aware.
- [ ] Optional infrastructure is added only when evidence justifies it.

---

# Suggested Daily Commit Cadence

A healthy active-day sequence may contain:

1. A type, interface, or schema foundation.
2. Validation or internal behavior.
3. Integration into one code path.
4. Edge-case or security hardening.
5. Related configuration, migration, or substantive documentation.

Tests required for a behavior belong in the same commit as that behavior. Never
use empty commits, WIP commits, broken builds, test-later commits, or formatting-
only commits created solely to meet the daily target.

# Initial Execution Order

1. Complete Stage 0 repository baseline and CI.
2. Establish the RPC test infrastructure.
3. Modularize MCP tool declarations and execution.
4. Introduce the structured error contract.
5. Harden the RPC client and add safe read-only retry.
6. Complete query protection and the production read-only core.
7. Add identity, secret handling, and policy foundations.
8. Build the safe mutation lifecycle.
9. Continue with audit, semantics, business tools, and enterprise capabilities.

Safe write work must not bypass the typed-error, test, identity, and policy
foundations.

# Open Decisions

- [ ] D-01 — Select the minimum supported Odoo version.
- [x] D-02 — Define backward compatibility for version `0.3.0` configuration.
- [ ] D-03 — Select the first durable local store; SQLite is the leading candidate.
- [ ] D-04 — Decide whether the approval UI is P0 or follows protocol stabilization.
- [ ] D-05 — Select the first remote MCP transport and deployment model.
- [ ] D-06 — Define audit retention and sensitive-data policy.
- [ ] D-07 — Select the first supported remote authentication method.
- [ ] D-08 — Confirm that the five-commit target applies to active workdays.

# Decision Log

Use this format for approved decisions:

```text
YYYY-MM-DD — D-XX — Decision title

Decision:
The selected option.

Reason:
The primary reasoning and trade-offs.

Impact:
Affected stages, configuration, or compatibility.
```

```text
2026-09-01 — D-02 — Preserve 0.3.0 configuration compatibility

Decision:
Version 0.3.1 continues to accept existing 0.3.0 configuration files without a
migration step.

Reason:
The release changes runtime validation and internal architecture but does not
change the serialized configuration schema.

Impact:
Existing deployments can replace the binary in place. Tool calls with malformed
IDs, write values, or flat domains may now be rejected earlier.
```

# Progress Summary

| Stage | Status | Completed | Notes |
|---|---|---:|---|
| Stage 0 — Baseline Engineering and Testability | Completed | 23/23 | All quality gates pass. |
| Stage 1 — Structured Errors and RPC Reliability | In progress | 3/29 | |
| Stage 2 — Query Protection and Safe Read-Only Core | Not started | 0/24 | |
| Stage 3 — Identity, Secrets, and Authorization | Not started | 0/29 | |
| Stage 4 — Safe Mutation Lifecycle | Not started | 0/59 | |
| Stage 5 — Audit and Observability | Not started | 0/26 | |
| Stage 6 — Odoo Semantic and Workflow Layer | Not started | 0/37 | |
| Stage 7 — Business Tools and Analytics | Not started | 0/28 | |
| Stage 8 — Remote MCP and Enterprise Readiness | Not started | 0/22 | |
| Stage 9 — Strategic and Optional Capabilities | Not started | 0/35 | |
