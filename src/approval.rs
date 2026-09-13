#![allow(dead_code)] // Approval persistence is integrated incrementally through Section 4.4.

use crate::context::RequestContext;
use crate::error::AppError;
use crate::operation::Operation;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub(crate) struct ApprovalId(Uuid);

impl ApprovalId {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for ApprovalId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApprovalState {
    Pending,
    Approved,
    Rejected,
    Expired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct ApprovalRequest {
    pub(crate) id: ApprovalId,
    pub(crate) operation_id: String,
    pub(crate) payload_hash: String,
    pub(crate) actor_subject: Option<String>,
    pub(crate) instance: String,
    pub(crate) requested_at: i64,
    pub(crate) expires_at: i64,
    pub(crate) state: ApprovalState,
}

impl ApprovalRequest {
    pub(crate) fn for_operation(
        operation: &Operation,
        context: &RequestContext,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<Self, AppError> {
        if ttl_seconds <= 0 {
            return Err(AppError::input_validation(
                "Approval expiration must be greater than zero",
            ));
        }
        Ok(Self {
            id: ApprovalId::new(),
            operation_id: operation.id.to_string(),
            payload_hash: operation.payload_hash.to_string(),
            actor_subject: context.actor.subject.clone(),
            instance: context.instance.clone(),
            requested_at: now,
            expires_at: now.saturating_add(ttl_seconds),
            state: ApprovalState::Pending,
        })
    }

    pub(crate) fn is_expired(&self, now: i64) -> bool {
        now >= self.expires_at
    }

    pub(crate) fn ensure_matches(
        &self,
        operation: &Operation,
        context: &RequestContext,
    ) -> Result<(), AppError> {
        if self.operation_id != operation.id.to_string() {
            return Err(AppError::authorization(
                "Approval is bound to another operation",
            ));
        }
        if self.payload_hash != operation.payload_hash.to_string() {
            return Err(AppError::authorization(
                "Operation payload changed after approval",
            ));
        }
        if self.actor_subject != context.actor.subject || self.instance != context.instance {
            return Err(AppError::authorization(
                "Approval actor or instance does not match the request",
            ));
        }
        Ok(())
    }
}

pub(crate) trait ApprovalStorage {
    fn insert(&mut self, request: ApprovalRequest) -> Result<(), AppError>;
    fn get(&self, id: ApprovalId) -> Result<Option<ApprovalRequest>, AppError>;
    fn update(&mut self, request: ApprovalRequest) -> Result<(), AppError>;
}

#[derive(Debug)]
pub(crate) struct LocalApprovalStorage {
    path: PathBuf,
    records: BTreeMap<ApprovalId, ApprovalRequest>,
}

impl LocalApprovalStorage {
    pub(crate) fn open(path: impl Into<PathBuf>) -> Result<Self, AppError> {
        let path = path.into();
        let records = if path.exists() {
            let bytes = fs::read(&path).map_err(|error| AppError::internal(error.to_string()))?;
            serde_json::from_slice(&bytes).map_err(|error| AppError::internal(error.to_string()))?
        } else {
            BTreeMap::new()
        };
        Ok(Self { path, records })
    }

    fn persist(&self) -> Result<(), AppError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| AppError::internal(error.to_string()))?;
        }
        let bytes = serde_json::to_vec_pretty(&self.records)
            .map_err(|error| AppError::internal(error.to_string()))?;
        let temporary = self.path.with_extension("tmp");
        fs::write(&temporary, bytes).map_err(|error| AppError::internal(error.to_string()))?;
        fs::rename(&temporary, &self.path).map_err(|error| AppError::internal(error.to_string()))
    }
}

impl ApprovalStorage for LocalApprovalStorage {
    fn insert(&mut self, request: ApprovalRequest) -> Result<(), AppError> {
        self.records.insert(request.id, request);
        self.persist()
    }

    fn get(&self, id: ApprovalId) -> Result<Option<ApprovalRequest>, AppError> {
        Ok(self.records.get(&id).cloned())
    }

    fn update(&mut self, request: ApprovalRequest) -> Result<(), AppError> {
        if !self.records.contains_key(&request.id) {
            return Err(AppError::input_validation(
                "Approval request does not exist",
            ));
        }
        self.records.insert(request.id, request);
        self.persist()
    }
}

impl ApprovalRequest {
    pub(crate) fn approve(&mut self) -> Result<(), AppError> {
        if self.state != ApprovalState::Pending {
            return Err(AppError::authorization(
                "Only pending approvals can be approved",
            ));
        }
        self.state = ApprovalState::Approved;
        Ok(())
    }

    pub(crate) fn reject(&mut self) -> Result<(), AppError> {
        if self.state != ApprovalState::Pending {
            return Err(AppError::authorization(
                "Only pending approvals can be rejected",
            ));
        }
        self.state = ApprovalState::Rejected;
        Ok(())
    }
}

fn _path_is_supported(path: &Path) -> bool {
    path.extension().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ActorIdentity, ClientIdentity, IdentitySource, RequestContext};
    use crate::operation::{Operation, OperationKind, OperationPayload};
    use serde_json::json;

    fn context(subject: Option<&str>, instance: &str) -> RequestContext {
        RequestContext::identified(
            ClientIdentity::default(),
            Default::default(),
            ActorIdentity::claimed(
                subject.map(str::to_string),
                None,
                IdentitySource::AuthenticatedTransport,
            ),
            instance.to_string(),
        )
    }

    fn operation() -> Operation {
        Operation::new(
            OperationKind::Update,
            "res.partner",
            OperationPayload::new(json!({"ids": [1], "vals": {"name": "A"}})).unwrap(),
        )
    }

    #[test]
    fn approval_schema_binds_operation_and_identity_with_expiration() {
        let request =
            ApprovalRequest::for_operation(&operation(), &context(Some("u1"), "prod"), 100, 60)
                .unwrap();
        assert_eq!(request.state, ApprovalState::Pending);
        assert_eq!(request.expires_at, 160);
        assert!(request.is_expired(160));
    }

    #[test]
    fn approval_rejects_payload_actor_and_instance_changes() {
        let op = operation();
        let owner = context(Some("u1"), "prod");
        let request = ApprovalRequest::for_operation(&op, &owner, 100, 60).unwrap();
        assert!(request.ensure_matches(&op, &owner).is_ok());
        assert!(request.ensure_matches(&operation(), &owner).is_err());
        assert!(
            request
                .ensure_matches(&op, &context(Some("u2"), "prod"))
                .is_err()
        );
        assert!(
            request
                .ensure_matches(&op, &context(Some("u1"), "test"))
                .is_err()
        );
    }

    #[test]
    fn local_storage_survives_reopen() {
        let path = std::env::temp_dir().join(format!("odoo-approval-{}.json", Uuid::new_v4()));
        let request =
            ApprovalRequest::for_operation(&operation(), &context(None, "prod"), 1, 10).unwrap();
        let id = request.id;
        let mut storage = LocalApprovalStorage::open(&path).unwrap();
        storage.insert(request.clone()).unwrap();
        drop(storage);
        let reopened = LocalApprovalStorage::open(&path).unwrap();
        assert_eq!(reopened.get(id).unwrap(), Some(request));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn approval_state_transitions_are_guarded() {
        let mut request =
            ApprovalRequest::for_operation(&operation(), &context(None, "prod"), 1, 10).unwrap();
        request.approve().unwrap();
        assert_eq!(request.state, ApprovalState::Approved);
        assert!(request.reject().is_err());
    }
}
