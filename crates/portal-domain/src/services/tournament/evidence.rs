//! Evidence service.
//!
//! Handles evidence management including uploads, external links,
//! access URL generation, and evidence discovery integration.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use portal_core::{DomainError, EvidenceId, TournamentMatchId, TournamentRegistrationId, UserId};
use tracing::{info, instrument, warn};

use crate::entities::evidence::{
    DiscoveredEvidence, Evidence, EvidenceAccessType, EvidenceAccessUrl, EvidenceSource,
    EvidenceStatus, EvidenceStorage, EvidenceType, EvidenceUploadInfo, EvidenceValidation,
    MatchEvidenceContext,
};
use crate::entities::result_claim::GameResult;
use crate::repositories::evidence::{CreateEvidence, CreateEvidenceAccessLog, EvidenceRepository};
use crate::repositories::tournament::{
    TournamentMatchRepository, TournamentRegistrationRepository,
};

/// S3 client trait for presigned URLs.
///
/// This trait abstracts the S3 operations needed by the evidence service.
/// It matches the S3EvidenceClient trait in portal-storage.
#[async_trait::async_trait]
pub trait EvidenceS3Client: Send + Sync + 'static {
    /// Generate a presigned PUT URL for uploading.
    async fn presign_put(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        ttl: Duration,
    ) -> Result<String, DomainError>;

    /// Generate a presigned GET URL for downloading.
    async fn presign_get(
        &self,
        bucket: &str,
        key: &str,
        ttl: Duration,
    ) -> Result<String, DomainError>;

    /// Check if an object exists.
    async fn object_exists(&self, bucket: &str, key: &str) -> Result<bool, DomainError>;

    /// Delete an object.
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), DomainError>;
}

/// Evidence plugin trait for discovery and validation.
#[async_trait::async_trait]
pub trait EvidencePluginClient: Send + Sync + 'static {
    /// Discover available evidence for a match.
    async fn discover_evidence(
        &self,
        context: &MatchEvidenceContext,
    ) -> Result<Vec<DiscoveredEvidence>, DomainError>;

    /// Validate evidence against a claimed result.
    async fn validate_evidence(
        &self,
        evidence: &Evidence,
        claimed_result: &GameResult,
    ) -> Result<EvidenceValidation, DomainError>;
}

/// Configuration for the evidence service.
#[derive(Debug, Clone)]
pub struct EvidenceServiceConfig {
    /// S3 bucket for evidence storage.
    pub evidence_bucket: String,
    /// Default retention period in days.
    pub default_retention_days: i64,
    /// Maximum file size in bytes.
    pub max_file_size_bytes: i64,
    /// Upload URL TTL in seconds.
    pub upload_url_ttl_seconds: u64,
    /// Access URL TTL in seconds.
    pub access_url_ttl_seconds: u64,
}

impl Default for EvidenceServiceConfig {
    fn default() -> Self {
        Self {
            evidence_bucket: "evidence".to_string(),
            default_retention_days: 90,
            max_file_size_bytes: 500 * 1024 * 1024, // 500 MB
            upload_url_ttl_seconds: 3600,           // 1 hour
            access_url_ttl_seconds: 3600,           // 1 hour
        }
    }
}

/// Service for managing match evidence.
#[derive(Clone)]
pub struct EvidenceService<ER, TMR, TRR, S3C> {
    evidence_repo: Arc<ER>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    s3_client: Arc<S3C>,
    config: EvidenceServiceConfig,
}

impl<ER, TMR, TRR, S3C> EvidenceService<ER, TMR, TRR, S3C>
where
    ER: EvidenceRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    S3C: EvidenceS3Client,
{
    /// Create a new evidence service.
    pub fn new(
        evidence_repo: Arc<ER>,
        match_repo: Arc<TMR>,
        registration_repo: Arc<TRR>,
        s3_client: Arc<S3C>,
        config: EvidenceServiceConfig,
    ) -> Self {
        Self {
            evidence_repo,
            match_repo,
            registration_repo,
            s3_client,
            config,
        }
    }

    /// Initiate an evidence upload.
    ///
    /// Returns presigned URL information for the client to upload directly to S3.
    /// The evidence record is created with `Pending` status until `complete_upload()` is called.
    ///
    /// `s3_key_prefix` — optional human-readable prefix built by the handler from
    /// league/tournament slugs. Falls back to UUID-based key if `None`.
    #[instrument(skip(self))]
    pub async fn initiate_upload(
        &self,
        match_id: TournamentMatchId,
        s3_key_prefix: Option<String>,
        game_number: Option<i32>,
        evidence_type: EvidenceType,
        file_name: String,
        file_size_bytes: i64,
        mime_type: String,
        uploaded_by: UserId,
    ) -> Result<EvidenceUploadInfo, DomainError> {
        // Validate file size
        if file_size_bytes > self.config.max_file_size_bytes {
            return Err(DomainError::InvalidState(format!(
                "File size {} exceeds maximum allowed {} bytes",
                file_size_bytes, self.config.max_file_size_bytes
            )));
        }

        // Verify match exists
        let match_ = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(match_id))?;

        // Find user's registration
        let registration_id = self.find_user_registration(&match_, uploaded_by).await.ok();

        // Generate S3 key — use human-readable prefix if provided, else UUID-based
        let extension = file_name.rsplit('.').next().unwrap_or("bin");
        let evidence_id = EvidenceId::new();
        let s3_key = match s3_key_prefix {
            Some(prefix) => format!("{}/{}.{}", prefix, evidence_id, extension),
            None => format!(
                "evidence/{}/{}/{}.{}",
                match_.tournament_id, match_id, evidence_id, extension
            ),
        };

        // Calculate expiration
        let expires_at = Utc::now() + ChronoDuration::days(self.config.default_retention_days);

        // Create evidence record as Pending (not yet uploaded)
        let evidence = self
            .evidence_repo
            .create(CreateEvidence {
                match_id,
                game_number,
                evidence_type,
                evidence_source: EvidenceSource::ManualUpload,
                name: file_name.clone(),
                description: None,
                file_size_bytes: Some(file_size_bytes),
                mime_type: Some(mime_type.clone()),
                storage: EvidenceStorage::S3 {
                    bucket: self.config.evidence_bucket.clone(),
                    key: s3_key.clone(),
                },
                plugin_metadata: serde_json::json!({}),
                uploaded_by_registration_id: registration_id,
                uploaded_by_user_id: Some(uploaded_by),
                discovered_by_plugin: None,
                discovered_at: None,
                expires_at: Some(expires_at),
                status: Some(EvidenceStatus::Pending),
            })
            .await?;

        // Generate presigned upload URL
        let upload_url = self
            .s3_client
            .presign_put(
                &self.config.evidence_bucket,
                &s3_key,
                &mime_type,
                Duration::from_secs(self.config.upload_url_ttl_seconds),
            )
            .await?;

        let url_expires_at =
            Utc::now() + ChronoDuration::seconds(self.config.upload_url_ttl_seconds as i64);

        info!(
            evidence_id = %evidence.id,
            match_id = %match_id,
            file_name = %file_name,
            "Evidence upload initiated"
        );

        Ok(EvidenceUploadInfo {
            evidence_id: evidence.id,
            upload_url,
            upload_method: "PUT".to_string(),
            upload_headers: {
                let mut headers = HashMap::new();
                headers.insert("Content-Type".to_string(), mime_type);
                headers.insert("Content-Length".to_string(), file_size_bytes.to_string());
                headers
            },
            expires_at: url_expires_at,
        })
    }

    /// Complete an evidence upload (verify file was uploaded and transition to Active).
    #[instrument(skip(self))]
    pub async fn complete_upload(&self, evidence_id: EvidenceId) -> Result<Evidence, DomainError> {
        let evidence = self
            .evidence_repo
            .find_by_id(evidence_id)
            .await?
            .ok_or_else(|| DomainError::EvidenceNotFound(evidence_id))?;

        // Only pending evidence can be completed
        if evidence.status != EvidenceStatus::Pending {
            return Err(DomainError::InvalidState(format!(
                "Evidence {} is already {} (expected pending)",
                evidence_id, evidence.status
            )));
        }

        // Verify the file was actually uploaded
        if let EvidenceStorage::S3 { bucket, key } = &evidence.storage {
            let exists = self.s3_client.object_exists(bucket, key).await?;
            if !exists {
                return Err(DomainError::InvalidState(
                    "Evidence file not found in storage".to_string(),
                ));
            }
        }

        // Transition Pending → Active
        let updated = self
            .evidence_repo
            .update_status(evidence_id, EvidenceStatus::Active)
            .await?;

        info!(
            evidence_id = %evidence_id,
            "Evidence upload completed"
        );

        Ok(updated)
    }

    /// Add an external link as evidence.
    #[instrument(skip(self))]
    pub async fn add_link(
        &self,
        match_id: TournamentMatchId,
        game_number: Option<i32>,
        evidence_type: EvidenceType,
        url: String,
        name: String,
        description: Option<String>,
        added_by: UserId,
    ) -> Result<Evidence, DomainError> {
        // Validate evidence type allows URL storage
        if !matches!(evidence_type, EvidenceType::Video | EvidenceType::Link) {
            return Err(DomainError::InvalidState(format!(
                "Evidence type {evidence_type:?} cannot be a URL"
            )));
        }

        // Verify match exists
        let match_ = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(match_id))?;

        // Find user's registration
        let registration_id = self.find_user_registration(&match_, added_by).await.ok();

        // Calculate expiration
        let expires_at = Utc::now() + ChronoDuration::days(self.config.default_retention_days);

        let evidence = self
            .evidence_repo
            .create(CreateEvidence {
                match_id,
                game_number,
                evidence_type,
                evidence_source: EvidenceSource::ManualUpload,
                name,
                description,
                file_size_bytes: None,
                mime_type: None,
                storage: EvidenceStorage::Url { url },
                plugin_metadata: serde_json::json!({}),
                uploaded_by_registration_id: registration_id,
                uploaded_by_user_id: Some(added_by),
                discovered_by_plugin: None,
                discovered_at: None,
                expires_at: Some(expires_at),
                status: None, // Links are immediately Active
            })
            .await?;

        info!(
            evidence_id = %evidence.id,
            match_id = %match_id,
            "External link evidence added"
        );

        Ok(evidence)
    }

    /// Get a single evidence record by ID.
    #[instrument(skip(self))]
    pub async fn get_evidence(&self, id: EvidenceId) -> Result<Evidence, DomainError> {
        self.evidence_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::EvidenceNotFound(id))
    }

    /// Get all evidence for a match.
    #[instrument(skip(self))]
    pub async fn get_match_evidence(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<Evidence>, DomainError> {
        self.evidence_repo.find_by_match(match_id).await
    }

    /// Get evidence for a specific game in a match.
    #[instrument(skip(self))]
    pub async fn get_game_evidence(
        &self,
        match_id: TournamentMatchId,
        game_number: i32,
    ) -> Result<Vec<Evidence>, DomainError> {
        self.evidence_repo
            .find_by_match_and_game(match_id, game_number)
            .await
    }

    /// Get a presigned access URL for evidence.
    #[instrument(skip(self))]
    pub async fn get_access_url(
        &self,
        evidence_id: EvidenceId,
        accessed_by: UserId,
        ip_address: Option<std::net::IpAddr>,
        user_agent: Option<String>,
    ) -> Result<EvidenceAccessUrl, DomainError> {
        let evidence = self
            .evidence_repo
            .find_by_id(evidence_id)
            .await?
            .ok_or_else(|| DomainError::EvidenceNotFound(evidence_id))?;

        // Check if evidence is accessible
        if !evidence.is_accessible() {
            return Err(DomainError::InvalidState(format!(
                "Evidence {} is not accessible (status: {})",
                evidence_id, evidence.status
            )));
        }

        // Check expiration
        if evidence.is_expired() {
            return Err(DomainError::InvalidState(format!(
                "Evidence {evidence_id} has expired"
            )));
        }

        // Log access
        self.evidence_repo
            .log_access(CreateEvidenceAccessLog {
                evidence_id,
                accessed_by_user_id: Some(accessed_by),
                access_type: EvidenceAccessType::Download,
                ip_address,
                user_agent,
            })
            .await?;

        // Generate access URL based on storage type
        let (url, expires_at) = match &evidence.storage {
            EvidenceStorage::S3 { bucket, key } => {
                let presigned_url = self
                    .s3_client
                    .presign_get(
                        bucket,
                        key,
                        Duration::from_secs(self.config.access_url_ttl_seconds),
                    )
                    .await?;
                let expires =
                    Utc::now() + ChronoDuration::seconds(self.config.access_url_ttl_seconds as i64);
                (presigned_url, Some(expires))
            }
            EvidenceStorage::Url { url } => (url.clone(), None),
            EvidenceStorage::Inline { .. } => {
                // Inline content not supported for direct access URLs
                return Err(DomainError::InvalidState(
                    "Inline evidence does not support access URLs".to_string(),
                ));
            }
        };

        Ok(EvidenceAccessUrl {
            url,
            expires_at,
            content_type: evidence.mime_type,
        })
    }

    /// Delete evidence.
    #[instrument(skip(self))]
    pub async fn delete_evidence(
        &self,
        evidence_id: EvidenceId,
        deleted_by: UserId,
    ) -> Result<(), DomainError> {
        let evidence = self
            .evidence_repo
            .find_by_id(evidence_id)
            .await?
            .ok_or_else(|| DomainError::EvidenceNotFound(evidence_id))?;

        // Delete from storage if S3
        if let EvidenceStorage::S3 { bucket, key } = &evidence.storage {
            self.s3_client.delete_object(bucket, key).await?;
        }

        // Mark as deleted
        self.evidence_repo
            .update_status(evidence_id, EvidenceStatus::Deleted)
            .await?;

        info!(
            evidence_id = %evidence_id,
            deleted_by = %deleted_by,
            "Evidence deleted"
        );

        Ok(())
    }

    /// Process expired evidence.
    #[instrument(skip(self))]
    pub async fn process_expired(&self) -> Result<Vec<Evidence>, DomainError> {
        let now = Utc::now();
        let expired = self.evidence_repo.find_expired(now).await?;

        let mut processed = Vec::new();

        for evidence in expired {
            // Delete from storage if S3
            if let EvidenceStorage::S3 { bucket, key } = &evidence.storage {
                if let Err(e) = self.s3_client.delete_object(bucket, key).await {
                    warn!(
                        evidence_id = %evidence.id,
                        error = %e,
                        "Failed to delete expired evidence from S3"
                    );
                    continue;
                }
            }

            // Mark as expired
            if let Ok(updated) = self
                .evidence_repo
                .update_status(evidence.id, EvidenceStatus::Expired)
                .await
            {
                processed.push(updated);
            }
        }

        if !processed.is_empty() {
            info!(count = processed.len(), "Processed expired evidence");
        }

        Ok(processed)
    }

    /// Clean up stale pending evidence (abandoned uploads).
    ///
    /// Deletes any evidence that has been in `Pending` status for longer than
    /// the specified max age. Also removes the S3 object if one was partially uploaded.
    #[instrument(skip(self))]
    pub async fn cleanup_stale_pending(
        &self,
        max_age: ChronoDuration,
    ) -> Result<Vec<Evidence>, DomainError> {
        let cutoff = Utc::now() - max_age;
        let stale = self.evidence_repo.find_stale_pending(cutoff).await?;

        let mut cleaned = Vec::new();
        for evidence in stale {
            // Best-effort delete from storage
            if let EvidenceStorage::S3 { bucket, key } = &evidence.storage {
                let _ = self.s3_client.delete_object(bucket, key).await;
            }

            if let Ok(updated) = self
                .evidence_repo
                .update_status(evidence.id, EvidenceStatus::Deleted)
                .await
            {
                cleaned.push(updated);
            }
        }

        if !cleaned.is_empty() {
            info!(count = cleaned.len(), "Cleaned up stale pending evidence");
        }

        Ok(cleaned)
    }

    // =========================================================================
    // INTERNAL HELPERS
    // =========================================================================

    async fn find_user_registration(
        &self,
        match_: &crate::entities::TournamentMatch,
        user_id: UserId,
    ) -> Result<TournamentRegistrationId, DomainError> {
        // Check participant 1
        if let Some(reg_id) = match_.participant1_registration_id {
            if let Some(reg) = self.registration_repo.find_by_id(reg_id).await? {
                if reg.registered_by == user_id {
                    return Ok(reg_id);
                }
            }
        }

        // Check participant 2
        if let Some(reg_id) = match_.participant2_registration_id {
            if let Some(reg) = self.registration_repo.find_by_id(reg_id).await? {
                if reg.registered_by == user_id {
                    return Ok(reg_id);
                }
            }
        }

        Err(DomainError::NotAuthorized(
            "User is not a participant in this match".to_string(),
        ))
    }
}

/// Extension for evidence discovery integration.
impl<ER, TMR, TRR, S3C> EvidenceService<ER, TMR, TRR, S3C>
where
    ER: EvidenceRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    S3C: EvidenceS3Client,
{
    /// Discover available evidence for a match using a plugin.
    ///
    /// The caller is responsible for building the [`MatchEvidenceContext`] with
    /// the correct `game_id` and participant data (the handler layer has access
    /// to the game repository and registration services needed for this).
    #[instrument(skip(self, plugin))]
    pub async fn discover_available<P: EvidencePluginClient>(
        &self,
        match_id: TournamentMatchId,
        context: &MatchEvidenceContext,
        plugin: &P,
    ) -> Result<Vec<DiscoveredEvidence>, DomainError> {
        // Verify match exists
        let _match = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(match_id))?;

        plugin.discover_evidence(context).await
    }

    /// Link discovered evidence to a match.
    #[instrument(skip(self))]
    pub async fn link_discovered(
        &self,
        match_id: TournamentMatchId,
        discovered: DiscoveredEvidence,
        game_number: Option<i32>,
        linked_by: UserId,
    ) -> Result<Evidence, DomainError> {
        // Verify match exists
        let match_ = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(match_id))?;

        // Find user's registration
        let registration_id = self.find_user_registration(&match_, linked_by).await.ok();

        // Calculate expiration
        let expires_at = Utc::now() + ChronoDuration::days(self.config.default_retention_days);

        let evidence = self
            .evidence_repo
            .create(CreateEvidence {
                match_id,
                game_number,
                evidence_type: discovered.evidence_type,
                evidence_source: EvidenceSource::PluginDiscovery,
                name: discovered.name,
                description: None,
                file_size_bytes: discovered.file_size_bytes,
                mime_type: None,
                storage: discovered.storage,
                plugin_metadata: discovered.metadata,
                uploaded_by_registration_id: registration_id,
                uploaded_by_user_id: Some(linked_by),
                discovered_by_plugin: None, // Plugin ID would come from context
                discovered_at: Some(discovered.discovered_at),
                expires_at: Some(expires_at),
                status: None, // Discovered evidence is immediately Active
            })
            .await?;

        info!(
            evidence_id = %evidence.id,
            match_id = %match_id,
            external_id = %discovered.external_id,
            "Discovered evidence linked"
        );

        Ok(evidence)
    }

    /// Validate evidence against a claimed result.
    #[instrument(skip(self, plugin))]
    pub async fn validate_against_result<P: EvidencePluginClient>(
        &self,
        evidence_id: EvidenceId,
        result: &GameResult,
        plugin: &P,
    ) -> Result<EvidenceValidation, DomainError> {
        let evidence = self
            .evidence_repo
            .find_by_id(evidence_id)
            .await?
            .ok_or_else(|| DomainError::EvidenceNotFound(evidence_id))?;

        let validation = plugin.validate_evidence(&evidence, result).await?;

        // Update evidence with validation result
        self.evidence_repo
            .mark_validated(
                evidence_id,
                serde_json::to_value(&validation).unwrap_or_default(),
            )
            .await?;

        Ok(validation)
    }
}
