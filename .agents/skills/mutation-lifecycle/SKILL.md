---
name: mutation-lifecycle
description: Use when adding or changing Odoo create, update, copy, delete, batch, or workflow mutations. Enforces preview, risk classification, approval, idempotency, single execution, post-action verification, and audit safeguards.
---

# Mutation Lifecycle

`AGENTS.md` is the canonical rule set. Every mutating path must preserve the
following order and guarantees.

## 1. Classify the operation

- Build an `Operation` with the correct `OperationKind`, `OperationClass`, and
  normalized `OperationPayload` from `src/lifecycle/operation.rs`.
- Use `RiskEvaluator` from `src/risk.rs`; do not invent a risk level.
- Protected classes (workflow, financial, admin) have non-downgradable floors.
- Bulk operations escalate by threshold.

## 2. Preview without writing

- Use `preview_create`, `preview_update`, or `preview_delete` from
  `src/lifecycle/preview.rs`.
- `odoo-preview` must never call a mutation RPC.
- Return affected records, proposed values, bulk summaries, estimates, and
  warnings; mark computed or defaulted fields as estimates.

## 3. Require approval when risk demands it

- Use `ApprovalRequest::for_operation`, `request_approval`,
  `consume_approval`, and the approval storage in
  `src/lifecycle/approval.rs`.
- Bind approvals to actor, instance, operation, and payload hash.
- Enforce expiration, single use, retry-match rules, and state transitions.
- Rejected, expired, mismatched, or already-consumed approvals never execute.
- Never weaken approval requirements to make a test pass.

## 4. Make execution idempotent

- Use `IdempotencyKey`, `IdempotencyRecord`, and the storage contract in
  `src/lifecycle/idempotency.rs`.
- Scope records by actor and instance; bind them to the payload hash.
- Reject key reuse with a different payload.
- Treat in-flight duplicates as conflicts, not retries.
- Record post-send timeouts and ambiguous outcomes as unknown; never retry a
  mutation blindly.

## 5. Execute once

- Route through `OperationExecutor::execute`; do not call mutation RPCs from
  the MCP layer.
- Retries are allowed only for read-only operations (see `src/odoo/retry.rs`).
- Preserve `AppError` categories; never string-match Odoo failures.

## 6. Verify and audit

- Use `verify_create`, `verify_update`, and `verify_delete` from
  `src/lifecycle/verification.rs`.
- Keep execution status and verification status separate, including partial
  coverage, mismatch, and unknown outcomes.
- Record `AuditEvent` entries for execution, verification, approval decisions,
  and failures without secrets.
- Return a truthful `LifecycleOutcome`; do not report success on unverified
  mutations.

## 7. Tests

Add mocked-server tests for:

- risk floors and bulk escalation;
- preview never writes;
- approval binding, expiration, rejection, single use, and retry matching;
- idempotency replay, payload mismatch, concurrent duplicates, and unknown
  post-send states;
- verification success, mismatch, partial coverage, and deleted absence;
- audit redaction and hash-chain integrity.

Run the full quality gate from `AGENTS.md` before committing.
