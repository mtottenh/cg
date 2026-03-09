//! PostgreSQL implementation of EvidenceRepository.

use crate::entities::{EvidenceAccessLogRow, EvidenceRow, NewEvidence, NewEvidenceAccessLog};
use crate::DbPool;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::{DomainError, EvidenceId, TournamentMatchId, TournamentRegistrationId, UserId};
use portal_domain::entities::evidence::{
    Evidence, EvidenceAccessLog, EvidenceAccessType, EvidenceSource, EvidenceStatus,
    EvidenceStorage, EvidenceType,
};
use portal_domain::repositories::evidence::{
    CreateEvidence, CreateEvidenceAccessLog, EvidenceRepository,
    UpdateEvidence as DomainUpdateEvidence,
};
use std::net::IpAddr;

// =============================================================================
// EVIDENCE REPOSITORY
// =============================================================================

/// PostgreSQL implementation of EvidenceRepository.
#[derive(Debug, Clone)]
pub struct PgEvidenceRepository {
    pool: DbPool,
}

impl PgEvidenceRepository {
    /// Create a new repository instance.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EvidenceRepository for PgEvidenceRepository {
    async fn find_by_id(&self, id: EvidenceId) -> Result<Option<Evidence>, DomainError> {
        let row = sqlx::query_as::<_, EvidenceRow>(
            r"SELECT * FROM match_evidence WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find evidence: {e}")))?;

        row.map(evidence_row_to_domain).transpose()
    }

    async fn find_by_match(&self, match_id: TournamentMatchId) -> Result<Vec<Evidence>, DomainError> {
        let rows = sqlx::query_as::<_, EvidenceRow>(
            r"SELECT * FROM match_evidence WHERE match_id = $1 AND status NOT IN ('pending', 'deleted') ORDER BY created_at DESC",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find evidence by match: {e}")))?;

        rows.into_iter().map(evidence_row_to_domain).collect()
    }

    async fn find_by_match_and_game(
        &self,
        match_id: TournamentMatchId,
        game_number: i32,
    ) -> Result<Vec<Evidence>, DomainError> {
        let rows = sqlx::query_as::<_, EvidenceRow>(
            r"SELECT * FROM match_evidence WHERE match_id = $1 AND game_number = $2 AND status NOT IN ('pending', 'deleted') ORDER BY created_at DESC",
        )
        .bind(match_id.as_uuid())
        .bind(game_number)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find evidence by match and game: {e}")))?;

        rows.into_iter().map(evidence_row_to_domain).collect()
    }

    async fn create(&self, evidence: CreateEvidence) -> Result<Evidence, DomainError> {
        let (storage_type, storage_path, storage_bucket) = storage_to_db(&evidence.storage);
        let status = evidence.status.unwrap_or(EvidenceStatus::Active).to_string();

        let new_evidence = NewEvidence {
            match_id: evidence.match_id.as_uuid(),
            game_number: evidence.game_number,
            evidence_type: evidence.evidence_type.to_string(),
            evidence_source: evidence.evidence_source.to_string(),
            name: evidence.name,
            description: evidence.description,
            file_size_bytes: evidence.file_size_bytes,
            mime_type: evidence.mime_type,
            storage_type,
            storage_path,
            storage_bucket,
            plugin_metadata: evidence.plugin_metadata,
            uploaded_by_registration_id: evidence.uploaded_by_registration_id.map(|id| id.as_uuid()),
            uploaded_by_user_id: evidence.uploaded_by_user_id.map(|id| id.as_uuid()),
            discovered_by_plugin: evidence.discovered_by_plugin,
            discovered_at: evidence.discovered_at,
            expires_at: evidence.expires_at,
        };

        let row = sqlx::query_as::<_, EvidenceRow>(
            r"
            INSERT INTO match_evidence (
                match_id, game_number, evidence_type, evidence_source, name, description,
                file_size_bytes, mime_type, storage_type, storage_path, storage_bucket,
                plugin_metadata, uploaded_by_registration_id, uploaded_by_user_id,
                discovered_by_plugin, discovered_at, expires_at, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            RETURNING *
            ",
        )
        .bind(new_evidence.match_id)
        .bind(new_evidence.game_number)
        .bind(&new_evidence.evidence_type)
        .bind(&new_evidence.evidence_source)
        .bind(&new_evidence.name)
        .bind(&new_evidence.description)
        .bind(new_evidence.file_size_bytes)
        .bind(&new_evidence.mime_type)
        .bind(&new_evidence.storage_type)
        .bind(&new_evidence.storage_path)
        .bind(&new_evidence.storage_bucket)
        .bind(&new_evidence.plugin_metadata)
        .bind(new_evidence.uploaded_by_registration_id)
        .bind(new_evidence.uploaded_by_user_id)
        .bind(&new_evidence.discovered_by_plugin)
        .bind(new_evidence.discovered_at)
        .bind(new_evidence.expires_at)
        .bind(&status)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to create evidence: {e}")))?;

        evidence_row_to_domain(row)
    }

    async fn update(
        &self,
        id: EvidenceId,
        update: DomainUpdateEvidence,
    ) -> Result<Evidence, DomainError> {
        // Build dynamic update query
        let mut set_clauses = vec!["updated_at = NOW()".to_string()];
        let mut param_index = 2; // $1 is the id

        if update.name.is_some() {
            set_clauses.push(format!("name = ${param_index}"));
            param_index += 1;
        }
        if update.description.is_some() {
            set_clauses.push(format!("description = ${param_index}"));
            param_index += 1;
        }
        if update.file_size_bytes.is_some() {
            set_clauses.push(format!("file_size_bytes = ${param_index}"));
            param_index += 1;
        }
        if update.mime_type.is_some() {
            set_clauses.push(format!("mime_type = ${param_index}"));
            param_index += 1;
        }
        if update.storage.is_some() {
            set_clauses.push(format!("storage_type = ${param_index}"));
            param_index += 1;
            set_clauses.push(format!("storage_path = ${param_index}"));
            param_index += 1;
            set_clauses.push(format!("storage_bucket = ${param_index}"));
            param_index += 1;
        }
        if update.plugin_metadata.is_some() {
            set_clauses.push(format!("plugin_metadata = ${param_index}"));
            param_index += 1;
        }
        if update.status.is_some() {
            set_clauses.push(format!("status = ${param_index}"));
        }

        let query = format!(
            "UPDATE match_evidence SET {} WHERE id = $1 RETURNING *",
            set_clauses.join(", ")
        );

        let mut query_builder = sqlx::query_as::<_, EvidenceRow>(&query).bind(id.as_uuid());

        if let Some(name) = &update.name {
            query_builder = query_builder.bind(name);
        }
        if let Some(description) = &update.description {
            query_builder = query_builder.bind(description);
        }
        if let Some(file_size_bytes) = update.file_size_bytes {
            query_builder = query_builder.bind(file_size_bytes);
        }
        if let Some(mime_type) = &update.mime_type {
            query_builder = query_builder.bind(mime_type);
        }
        if let Some(storage) = &update.storage {
            let (storage_type, storage_path, storage_bucket) = storage_to_db(storage);
            query_builder = query_builder.bind(storage_type);
            query_builder = query_builder.bind(storage_path);
            query_builder = query_builder.bind(storage_bucket);
        }
        if let Some(plugin_metadata) = &update.plugin_metadata {
            query_builder = query_builder.bind(plugin_metadata);
        }
        if let Some(status) = &update.status {
            query_builder = query_builder.bind(status.to_string());
        }

        let row = query_builder
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to update evidence: {e}")))?;

        evidence_row_to_domain(row)
    }

    async fn update_status(
        &self,
        id: EvidenceId,
        status: EvidenceStatus,
    ) -> Result<Evidence, DomainError> {
        let row = sqlx::query_as::<_, EvidenceRow>(
            r"UPDATE match_evidence SET status = $2, updated_at = NOW() WHERE id = $1 RETURNING *",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to update evidence status: {e}")))?;

        evidence_row_to_domain(row)
    }

    async fn mark_validated(
        &self,
        id: EvidenceId,
        validation_result: serde_json::Value,
    ) -> Result<Evidence, DomainError> {
        let row = sqlx::query_as::<_, EvidenceRow>(
            r"UPDATE match_evidence SET validated = true, validated_at = NOW(), validation_result = $2, updated_at = NOW() WHERE id = $1 RETURNING *",
        )
        .bind(id.as_uuid())
        .bind(&validation_result)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to mark evidence validated: {e}")))?;

        evidence_row_to_domain(row)
    }

    async fn delete(&self, id: EvidenceId) -> Result<(), DomainError> {
        sqlx::query(
            r"UPDATE match_evidence SET status = 'deleted', updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to delete evidence: {e}")))?;

        Ok(())
    }

    async fn find_expired(&self, before: DateTime<Utc>) -> Result<Vec<Evidence>, DomainError> {
        let rows = sqlx::query_as::<_, EvidenceRow>(
            r"SELECT * FROM match_evidence WHERE expires_at IS NOT NULL AND expires_at < $1 AND status = 'active'",
        )
        .bind(before)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find expired evidence: {e}")))?;

        rows.into_iter().map(evidence_row_to_domain).collect()
    }

    async fn log_access(&self, log: CreateEvidenceAccessLog) -> Result<EvidenceAccessLog, DomainError> {
        let new_log = NewEvidenceAccessLog {
            evidence_id: log.evidence_id.as_uuid(),
            accessed_by_user_id: log.accessed_by_user_id.map(|id| id.as_uuid()),
            access_type: log.access_type.to_string(),
            ip_address: log.ip_address.map(|ip| ip.to_string()),
            user_agent: log.user_agent,
        };

        let row = sqlx::query_as::<_, EvidenceAccessLogRow>(
            r"
            INSERT INTO evidence_access_log (evidence_id, accessed_by_user_id, access_type, ip_address, user_agent)
            VALUES ($1, $2, $3, $4::inet, $5)
            RETURNING *
            ",
        )
        .bind(new_log.evidence_id)
        .bind(new_log.accessed_by_user_id)
        .bind(&new_log.access_type)
        .bind(&new_log.ip_address)
        .bind(&new_log.user_agent)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to log evidence access: {e}")))?;

        access_log_row_to_domain(row)
    }

    async fn get_access_log(&self, evidence_id: EvidenceId) -> Result<Vec<EvidenceAccessLog>, DomainError> {
        let rows = sqlx::query_as::<_, EvidenceAccessLogRow>(
            r"SELECT * FROM evidence_access_log WHERE evidence_id = $1 ORDER BY accessed_at DESC",
        )
        .bind(evidence_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to get evidence access log: {e}")))?;

        rows.into_iter().map(access_log_row_to_domain).collect()
    }

    async fn find_stale_pending(&self, created_before: DateTime<Utc>) -> Result<Vec<Evidence>, DomainError> {
        let rows = sqlx::query_as::<_, EvidenceRow>(
            r"SELECT * FROM match_evidence WHERE status = 'pending' AND created_at < $1",
        )
        .bind(created_before)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find stale pending evidence: {e}")))?;

        rows.into_iter().map(evidence_row_to_domain).collect()
    }
}

// =============================================================================
// CONVERSION FUNCTIONS
// =============================================================================

/// Convert an EvidenceRow to a domain Evidence.
fn evidence_row_to_domain(row: EvidenceRow) -> Result<Evidence, DomainError> {
    let evidence_type: EvidenceType = row
        .evidence_type
        .parse()
        .map_err(|e: String| DomainError::Internal(e))?;

    let evidence_source: EvidenceSource = row
        .evidence_source
        .parse()
        .map_err(|e: String| DomainError::Internal(e))?;

    let status: EvidenceStatus = row
        .status
        .parse()
        .map_err(|e: String| DomainError::Internal(e))?;

    let storage = db_to_storage(&row.storage_type, row.storage_path, row.storage_bucket)?;

    Ok(Evidence {
        id: EvidenceId::from_uuid(row.id),
        match_id: TournamentMatchId::from_uuid(row.match_id),
        game_number: row.game_number,
        evidence_type,
        evidence_source,
        name: row.name,
        description: row.description,
        file_size_bytes: row.file_size_bytes,
        mime_type: row.mime_type,
        storage,
        plugin_metadata: row.plugin_metadata,
        validated: row.validated,
        validated_at: row.validated_at,
        validation_result: row.validation_result,
        uploaded_by_registration_id: row.uploaded_by_registration_id.map(TournamentRegistrationId::from_uuid),
        uploaded_by_user_id: row.uploaded_by_user_id.map(UserId::from_uuid),
        discovered_by_plugin: row.discovered_by_plugin,
        discovered_at: row.discovered_at,
        status,
        created_at: row.created_at,
        updated_at: row.updated_at,
        expires_at: row.expires_at,
    })
}

/// Convert an EvidenceAccessLogRow to a domain EvidenceAccessLog.
fn access_log_row_to_domain(row: EvidenceAccessLogRow) -> Result<EvidenceAccessLog, DomainError> {
    let access_type: EvidenceAccessType = row
        .access_type
        .parse()
        .map_err(|e: String| DomainError::Internal(e))?;

    let ip_address: Option<IpAddr> = row
        .ip_address
        .map(|ip| ip.parse())
        .transpose()
        .map_err(|e| DomainError::Internal(format!("Invalid IP address: {e}")))?;

    Ok(EvidenceAccessLog {
        id: row.id,
        evidence_id: EvidenceId::from_uuid(row.evidence_id),
        accessed_by_user_id: row.accessed_by_user_id.map(UserId::from_uuid),
        access_type,
        ip_address,
        user_agent: row.user_agent,
        accessed_at: row.accessed_at,
    })
}

/// Convert EvidenceStorage to database columns.
fn storage_to_db(storage: &EvidenceStorage) -> (String, Option<String>, Option<String>) {
    match storage {
        EvidenceStorage::S3 { bucket, key } => {
            ("s3".to_string(), Some(key.clone()), Some(bucket.clone()))
        }
        EvidenceStorage::Url { url } => {
            ("url".to_string(), Some(url.clone()), None)
        }
        EvidenceStorage::Inline { content } => {
            ("inline".to_string(), Some(content.clone()), None)
        }
    }
}

/// Convert database columns to EvidenceStorage.
fn db_to_storage(
    storage_type: &str,
    storage_path: Option<String>,
    storage_bucket: Option<String>,
) -> Result<EvidenceStorage, DomainError> {
    match storage_type {
        "s3" => {
            let key = storage_path.ok_or_else(|| {
                DomainError::Internal("S3 storage missing key".to_string())
            })?;
            let bucket = storage_bucket.ok_or_else(|| {
                DomainError::Internal("S3 storage missing bucket".to_string())
            })?;
            Ok(EvidenceStorage::S3 { bucket, key })
        }
        "url" => {
            let url = storage_path.ok_or_else(|| {
                DomainError::Internal("URL storage missing URL".to_string())
            })?;
            Ok(EvidenceStorage::Url { url })
        }
        "inline" => {
            let content = storage_path.unwrap_or_default();
            Ok(EvidenceStorage::Inline { content })
        }
        _ => Err(DomainError::Internal(format!(
            "Unknown storage type: {storage_type}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_to_db_s3() {
        let storage = EvidenceStorage::S3 {
            bucket: "my-bucket".to_string(),
            key: "path/to/file.dem".to_string(),
        };
        let (storage_type, storage_path, storage_bucket) = storage_to_db(&storage);
        assert_eq!(storage_type, "s3");
        assert_eq!(storage_path, Some("path/to/file.dem".to_string()));
        assert_eq!(storage_bucket, Some("my-bucket".to_string()));
    }

    #[test]
    fn test_storage_to_db_url() {
        let storage = EvidenceStorage::Url {
            url: "https://example.com/video.mp4".to_string(),
        };
        let (storage_type, storage_path, storage_bucket) = storage_to_db(&storage);
        assert_eq!(storage_type, "url");
        assert_eq!(storage_path, Some("https://example.com/video.mp4".to_string()));
        assert_eq!(storage_bucket, None);
    }

    #[test]
    fn test_db_to_storage_s3() {
        let result = db_to_storage(
            "s3",
            Some("path/to/file.dem".to_string()),
            Some("my-bucket".to_string()),
        );
        assert!(result.is_ok());
        match result.unwrap() {
            EvidenceStorage::S3 { bucket, key } => {
                assert_eq!(bucket, "my-bucket");
                assert_eq!(key, "path/to/file.dem");
            }
            _ => panic!("Expected S3 storage"),
        }
    }

    #[test]
    fn test_db_to_storage_url() {
        let result = db_to_storage("url", Some("https://example.com/video.mp4".to_string()), None);
        assert!(result.is_ok());
        match result.unwrap() {
            EvidenceStorage::Url { url } => {
                assert_eq!(url, "https://example.com/video.mp4");
            }
            _ => panic!("Expected URL storage"),
        }
    }
}
