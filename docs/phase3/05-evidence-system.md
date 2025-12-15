# Evidence System Design

> **Sub-Phases**: 3.7 (Evidence System), 3.8 (Plugin Evidence Integration)
> **Related**: [04-result-submission.md](./04-result-submission.md), [07-disputes-forfeits.md](./07-disputes-forfeits.md)

---

## Overview

The Evidence System manages proof of match results and supports dispute resolution. Evidence can be uploaded manually or discovered automatically through game plugins (e.g., CS2 demo files stored in S3).

### Key Features

- **Multiple Evidence Types**: Demos, screenshots, external links
- **Plugin Integration**: Game-specific evidence discovery and validation
- **S3 Integration**: Presigned URLs for secure file access
- **Linkage**: Evidence linked to matches, games, and disputes
- **Retention Policies**: Configurable storage duration

---

## Evidence Types

| Type | Description | Source | Storage |
|------|-------------|--------|---------|
| `demo` | Game replay file | Plugin discovery, manual upload | S3 |
| `screenshot` | Result screenshot | Manual upload | S3 |
| `video` | VOD or clip | External link | URL reference |
| `link` | External resource | Manual entry | URL reference |
| `server_log` | Game server output | Plugin integration | S3 or inline |

---

## Database Schema

### match_evidence

```sql
CREATE TABLE match_evidence (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- What this evidence is for
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    game_number INTEGER,  -- NULL for match-level evidence

    -- Type and source
    evidence_type VARCHAR(32) NOT NULL,
    evidence_source VARCHAR(32) NOT NULL,

    -- Metadata
    name VARCHAR(256) NOT NULL,
    description TEXT,
    file_size_bytes BIGINT,
    mime_type VARCHAR(128),

    -- Storage location
    storage_type VARCHAR(32) NOT NULL,  -- 's3', 'url', 'inline'
    storage_path VARCHAR(512),          -- S3 key or URL
    storage_bucket VARCHAR(128),        -- S3 bucket name

    -- Plugin-provided metadata
    plugin_metadata JSONB NOT NULL DEFAULT '{}',

    -- Validation
    validated BOOLEAN NOT NULL DEFAULT false,
    validated_at TIMESTAMPTZ,
    validation_result JSONB,

    -- Uploaded by
    uploaded_by_registration_id UUID REFERENCES tournament_registrations(id),
    uploaded_by_user_id UUID REFERENCES users(id),

    -- Discovery source (for plugin-discovered evidence)
    discovered_by_plugin VARCHAR(64),
    discovered_at TIMESTAMPTZ,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'active',

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT match_evidence_check_type CHECK (evidence_type IN (
        'demo', 'screenshot', 'video', 'link', 'server_log'
    )),
    CONSTRAINT match_evidence_check_source CHECK (evidence_source IN (
        'manual_upload', 'plugin_discovery', 'game_server', 'external_api'
    )),
    CONSTRAINT match_evidence_check_storage CHECK (storage_type IN (
        's3', 'url', 'inline'
    )),
    CONSTRAINT match_evidence_check_status CHECK (status IN (
        'active', 'expired', 'deleted', 'quarantined'
    ))
);

CREATE INDEX idx_match_evidence_match ON match_evidence(match_id);
CREATE INDEX idx_match_evidence_match_game ON match_evidence(match_id, game_number);
CREATE INDEX idx_match_evidence_type ON match_evidence(evidence_type);
CREATE INDEX idx_match_evidence_expires ON match_evidence(expires_at)
    WHERE expires_at IS NOT NULL AND status = 'active';

COMMENT ON TABLE match_evidence IS 'Evidence files and links for match results';
```

### evidence_access_log

```sql
CREATE TABLE evidence_access_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    evidence_id UUID NOT NULL REFERENCES match_evidence(id) ON DELETE CASCADE,
    accessed_by_user_id UUID REFERENCES users(id),
    access_type VARCHAR(32) NOT NULL,  -- 'view', 'download', 'share'
    ip_address INET,
    user_agent TEXT,
    accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_evidence_access_log_evidence ON evidence_access_log(evidence_id);
CREATE INDEX idx_evidence_access_log_user ON evidence_access_log(accessed_by_user_id);

COMMENT ON TABLE evidence_access_log IS 'Audit log of evidence access';
```

---

## Domain Entities

### Evidence

```rust
use chrono::{DateTime, Utc};
use portal_core::ids::{
    EvidenceId, TournamentMatchId, TournamentRegistrationId, UserId,
};

/// Evidence item for a match.
#[derive(Debug, Clone)]
pub struct Evidence {
    pub id: EvidenceId,
    pub match_id: TournamentMatchId,
    pub game_number: Option<i32>,

    /// Type and source
    pub evidence_type: EvidenceType,
    pub evidence_source: EvidenceSource,

    /// Metadata
    pub name: String,
    pub description: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub mime_type: Option<String>,

    /// Storage
    pub storage: EvidenceStorage,

    /// Plugin metadata
    pub plugin_metadata: serde_json::Value,

    /// Validation
    pub validated: bool,
    pub validated_at: Option<DateTime<Utc>>,
    pub validation_result: Option<serde_json::Value>,

    /// Upload info
    pub uploaded_by_registration_id: Option<TournamentRegistrationId>,
    pub uploaded_by_user_id: Option<UserId>,

    /// Discovery info (for plugin-discovered)
    pub discovered_by_plugin: Option<String>,
    pub discovered_at: Option<DateTime<Utc>>,

    /// Status
    pub status: EvidenceStatus,

    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceType {
    Demo,
    Screenshot,
    Video,
    Link,
    ServerLog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceSource {
    ManualUpload,
    PluginDiscovery,
    GameServer,
    ExternalApi,
}

#[derive(Debug, Clone)]
pub enum EvidenceStorage {
    S3 {
        bucket: String,
        key: String,
    },
    Url {
        url: String,
    },
    Inline {
        content: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceStatus {
    Active,
    Expired,
    Deleted,
    Quarantined,  // Flagged for review
}

impl Evidence {
    /// Generate a presigned URL for S3 evidence.
    pub async fn get_access_url(&self, s3_client: &S3Client, ttl: Duration) -> Result<String, Error> {
        match &self.storage {
            EvidenceStorage::S3 { bucket, key } => {
                s3_client.presign_get(bucket, key, ttl).await
            }
            EvidenceStorage::Url { url } => Ok(url.clone()),
            EvidenceStorage::Inline { .. } => Err(Error::InlineContentNotUrl),
        }
    }
}
```

---

## Service Design

### EvidenceService

```rust
pub struct EvidenceService<ER, TMR, TRR, S3C>
where
    ER: EvidenceRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    S3C: S3Client,
{
    evidence_repo: Arc<ER>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    s3_client: Arc<S3C>,
    evidence_bucket: String,
    default_retention_days: i64,
    max_file_size_bytes: i64,
}

impl<ER, TMR, TRR, S3C> EvidenceService<ER, TMR, TRR, S3C>
where
    ER: EvidenceRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    S3C: S3Client,
{
    /// Upload evidence for a match.
    ///
    /// Returns upload URL (presigned) and evidence ID.
    pub async fn initiate_upload(
        &self,
        match_id: TournamentMatchId,
        game_number: Option<i32>,
        evidence_type: EvidenceType,
        file_name: String,
        file_size_bytes: i64,
        mime_type: String,
        uploaded_by: UserId,
    ) -> Result<EvidenceUploadInfo, DomainError>;

    /// Complete upload after file is uploaded to S3.
    pub async fn complete_upload(
        &self,
        evidence_id: EvidenceId,
    ) -> Result<Evidence, DomainError>;

    /// Add external link as evidence.
    pub async fn add_link(
        &self,
        match_id: TournamentMatchId,
        game_number: Option<i32>,
        evidence_type: EvidenceType,
        url: String,
        name: String,
        description: Option<String>,
        added_by: UserId,
    ) -> Result<Evidence, DomainError>;

    /// Link discovered evidence to a match.
    pub async fn link_discovered_evidence(
        &self,
        match_id: TournamentMatchId,
        game_number: Option<i32>,
        discovered_evidence: DiscoveredEvidence,
        linked_by: UserId,
    ) -> Result<Evidence, DomainError>;

    /// Get evidence for a match.
    pub async fn get_match_evidence(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<Evidence>, DomainError>;

    /// Get access URL for evidence.
    ///
    /// Generates presigned URL for S3, returns direct URL otherwise.
    /// Logs access.
    pub async fn get_access_url(
        &self,
        evidence_id: EvidenceId,
        accessed_by: UserId,
    ) -> Result<EvidenceAccessUrl, DomainError>;

    /// Delete evidence.
    pub async fn delete_evidence(
        &self,
        evidence_id: EvidenceId,
        deleted_by: UserId,
    ) -> Result<(), DomainError>;

    /// Process expired evidence.
    ///
    /// Called by background job.
    pub async fn process_expired(&self) -> Result<Vec<Evidence>, DomainError>;
}

#[derive(Debug, Clone)]
pub struct EvidenceUploadInfo {
    pub evidence_id: EvidenceId,
    pub upload_url: String,
    pub upload_method: String,  // "PUT"
    pub upload_headers: HashMap<String, String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct EvidenceAccessUrl {
    pub url: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub content_type: Option<String>,
}
```

---

## Plugin Integration

### TournamentPlugin Extension for Evidence

```rust
/// Extended plugin trait for evidence discovery.
pub trait EvidencePlugin: TournamentPlugin {
    /// Discover available evidence for a match.
    ///
    /// Scans external sources (S3 bucket, game server API) for evidence
    /// that might be related to this match.
    async fn discover_evidence(
        &self,
        match_context: &MatchContext,
    ) -> Result<Vec<DiscoveredEvidence>, PluginError>;

    /// Validate evidence matches the claimed result.
    ///
    /// For demos, can parse the file to extract actual scores.
    async fn validate_evidence(
        &self,
        evidence: &Evidence,
        claimed_result: &GameResult,
    ) -> Result<EvidenceValidation, PluginError>;

    /// Get demo file metadata without downloading.
    async fn get_demo_metadata(
        &self,
        storage_path: &str,
    ) -> Result<DemoMetadata, PluginError>;
}

#[derive(Debug, Clone)]
pub struct MatchContext {
    pub tournament_id: TournamentId,
    pub match_id: TournamentMatchId,
    pub game_id: GameId,
    pub participants: Vec<ParticipantContext>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ParticipantContext {
    pub registration_id: TournamentRegistrationId,
    pub name: String,
    pub player_ids: Vec<PlayerId>,  // For matching demos
    pub steam_ids: Vec<String>,     // Game-specific IDs
}

#[derive(Debug, Clone)]
pub struct DiscoveredEvidence {
    pub external_id: String,
    pub evidence_type: EvidenceType,
    pub name: String,
    pub storage: EvidenceStorage,
    pub file_size_bytes: Option<i64>,
    pub metadata: serde_json::Value,
    pub discovered_at: DateTime<Utc>,
    pub relevance_score: f32,  // 0.0 - 1.0, how likely this is the right demo
}

#[derive(Debug, Clone)]
pub struct EvidenceValidation {
    pub is_valid: bool,
    pub confidence: f32,
    pub extracted_result: Option<ExtractedResult>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ExtractedResult {
    pub map_id: String,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub duration_seconds: i64,
    pub player_stats: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct DemoMetadata {
    pub map_name: String,
    pub duration_seconds: i64,
    pub player_count: u32,
    pub team1_score: i32,
    pub team2_score: i32,
    pub recorded_at: DateTime<Utc>,
    pub server_name: Option<String>,
    pub demo_version: String,
}
```

### CS2 Plugin Evidence Implementation

```rust
impl EvidencePlugin for Cs2Plugin {
    async fn discover_evidence(
        &self,
        match_context: &MatchContext,
    ) -> Result<Vec<DiscoveredEvidence>, PluginError> {
        let mut discovered = Vec::new();

        // Scan S3 bucket for demos
        let demo_bucket = self.config.demo_bucket.as_ref()
            .ok_or(PluginError::ConfigMissing("demo_bucket"))?;

        // List objects with prefix based on match context
        let prefix = format!(
            "demos/{}/",
            match_context.scheduled_at
                .map(|d| d.format("%Y/%m/%d").to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );

        let objects = self.s3_client.list_objects(demo_bucket, &prefix).await?;

        for obj in objects {
            // Try to extract metadata from demo
            if let Ok(metadata) = self.get_demo_metadata(&obj.key).await {
                // Check if demo matches our match
                let relevance = self.calculate_demo_relevance(match_context, &metadata);

                if relevance > 0.5 {
                    discovered.push(DiscoveredEvidence {
                        external_id: obj.key.clone(),
                        evidence_type: EvidenceType::Demo,
                        name: obj.key.split('/').last().unwrap_or("demo.dem").to_string(),
                        storage: EvidenceStorage::S3 {
                            bucket: demo_bucket.clone(),
                            key: obj.key,
                        },
                        file_size_bytes: Some(obj.size),
                        metadata: serde_json::to_value(&metadata)?,
                        discovered_at: Utc::now(),
                        relevance_score: relevance,
                    });
                }
            }
        }

        // Sort by relevance
        discovered.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());

        Ok(discovered)
    }

    async fn validate_evidence(
        &self,
        evidence: &Evidence,
        claimed_result: &GameResult,
    ) -> Result<EvidenceValidation, PluginError> {
        // Only validate demos
        if evidence.evidence_type != EvidenceType::Demo {
            return Ok(EvidenceValidation {
                is_valid: true,
                confidence: 0.5,
                extracted_result: None,
                warnings: vec!["Non-demo evidence not validated".to_string()],
                errors: vec![],
            });
        }

        // Parse demo file
        let demo_data = match &evidence.storage {
            EvidenceStorage::S3 { bucket, key } => {
                self.s3_client.get_object(bucket, key).await?
            }
            _ => return Err(PluginError::UnsupportedStorage),
        };

        let parsed = self.demo_parser.parse(&demo_data)?;

        // Extract result from demo
        let extracted = ExtractedResult {
            map_id: parsed.map_name.clone(),
            participant1_score: parsed.team1_score,
            participant2_score: parsed.team2_score,
            duration_seconds: parsed.duration_seconds,
            player_stats: parsed.player_stats.clone(),
        };

        // Compare with claimed result
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        if extracted.map_id != claimed_result.map_id {
            errors.push(format!(
                "Map mismatch: demo has {}, claimed {}",
                extracted.map_id, claimed_result.map_id
            ));
        }

        // Score comparison (account for side swap in demo)
        let scores_match =
            (extracted.participant1_score == claimed_result.participant1_score
                && extracted.participant2_score == claimed_result.participant2_score)
            || (extracted.participant1_score == claimed_result.participant2_score
                && extracted.participant2_score == claimed_result.participant1_score);

        if !scores_match {
            errors.push(format!(
                "Score mismatch: demo has {}-{}, claimed {}-{}",
                extracted.participant1_score,
                extracted.participant2_score,
                claimed_result.participant1_score,
                claimed_result.participant2_score
            ));
        }

        let is_valid = errors.is_empty();
        let confidence = if is_valid { 0.95 } else { 0.2 };

        Ok(EvidenceValidation {
            is_valid,
            confidence,
            extracted_result: Some(extracted),
            warnings,
            errors,
        })
    }

    async fn get_demo_metadata(
        &self,
        storage_path: &str,
    ) -> Result<DemoMetadata, PluginError> {
        // Parse demo header without downloading full file
        let header = self.s3_client
            .get_object_range(&self.config.demo_bucket.unwrap(), storage_path, 0..4096)
            .await?;

        let metadata = self.demo_parser.parse_header(&header)?;

        Ok(DemoMetadata {
            map_name: metadata.map_name,
            duration_seconds: metadata.duration_ticks / 64,  // Assuming 64 tick
            player_count: metadata.player_count,
            team1_score: metadata.team1_score,
            team2_score: metadata.team2_score,
            recorded_at: metadata.recorded_at,
            server_name: metadata.server_name,
            demo_version: metadata.demo_version,
        })
    }
}
```

---

## API Endpoints

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/evidence/upload

Initiate evidence upload.

**Request**:
```json
{
  "game_number": 1,
  "evidence_type": "screenshot",
  "file_name": "game1_result.png",
  "file_size_bytes": 245678,
  "mime_type": "image/png"
}
```

**Response** (200 OK):
```json
{
  "data": {
    "evidence_id": "...",
    "upload_url": "https://s3.amazonaws.com/bucket/evidence/...",
    "upload_method": "PUT",
    "upload_headers": {
      "Content-Type": "image/png",
      "x-amz-acl": "private"
    },
    "expires_at": "2025-01-15T21:15:00Z"
  }
}
```

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/evidence/upload/complete

Complete upload after file uploaded to S3.

**Request**:
```json
{
  "evidence_id": "..."
}
```

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/evidence/link

Add external link as evidence.

**Request**:
```json
{
  "game_number": null,
  "evidence_type": "video",
  "url": "https://twitch.tv/videos/123456789",
  "name": "Match VOD",
  "description": "Full match recording"
}
```

### GET /v1/tournaments/{tournament_id}/matches/{match_id}/evidence/available

Get plugin-discovered evidence for selection.

**Response**:
```json
{
  "data": {
    "discovered": [
      {
        "external_id": "demos/2025/01/15/match_abc123.dem",
        "evidence_type": "demo",
        "name": "match_abc123.dem",
        "file_size_bytes": 45678901,
        "relevance_score": 0.92,
        "metadata": {
          "map_name": "de_mirage",
          "duration_seconds": 2450,
          "team1_score": 16,
          "team2_score": 12,
          "recorded_at": "2025-01-15T19:05:00Z"
        }
      }
    ],
    "last_scan_at": "2025-01-15T20:30:00Z"
  }
}
```

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/evidence/link-discovered

Link a discovered evidence item to the match.

**Request**:
```json
{
  "external_id": "demos/2025/01/15/match_abc123.dem",
  "game_number": 1
}
```

### GET /v1/tournaments/{tournament_id}/matches/{match_id}/evidence

Get all evidence for a match.

**Response**:
```json
{
  "data": {
    "evidence": [
      {
        "id": "...",
        "evidence_type": "demo",
        "game_number": 1,
        "name": "game1_demo.dem",
        "source": "plugin_discovery",
        "file_size_bytes": 45678901,
        "validated": true,
        "validation_result": {
          "is_valid": true,
          "extracted_scores": {"team1": 16, "team2": 12}
        },
        "created_at": "2025-01-15T20:35:00Z"
      },
      {
        "id": "...",
        "evidence_type": "screenshot",
        "game_number": 1,
        "name": "game1_result.png",
        "source": "manual_upload",
        "file_size_bytes": 245678,
        "validated": false,
        "created_at": "2025-01-15T20:40:00Z"
      }
    ]
  }
}
```

### GET /v1/tournaments/{tournament_id}/matches/{match_id}/evidence/{evidence_id}/access

Get access URL for evidence.

**Response**:
```json
{
  "data": {
    "url": "https://s3.amazonaws.com/bucket/evidence/...?X-Amz-...",
    "expires_at": "2025-01-15T21:45:00Z",
    "content_type": "application/octet-stream"
  }
}
```

---

## Access Control

### Evidence Visibility Rules

| Viewer | Own Match Evidence | Other Match Evidence | Expired |
|--------|-------------------|---------------------|---------|
| Match Participant | Full access | None | Read only |
| Tournament Admin | Full access | Full access | Read only |
| Public | If tournament public | None | None |
| Dispute Reviewer | Full access | Full access | Read only |

### Presigned URL Configuration

```rust
/// Evidence access URL configuration.
pub struct EvidenceAccessConfig {
    /// TTL for presigned URLs (default: 1 hour)
    pub presigned_url_ttl: Duration,

    /// Maximum file size for inline viewing (default: 50MB)
    pub max_inline_size: i64,

    /// Require re-authentication after this period
    pub access_reauth_period: Duration,
}
```

---

## Error Handling

### New Error Types

```rust
pub enum EvidenceError {
    /// Evidence not found
    NotFound(EvidenceId),

    /// File too large
    FileTooLarge { max: i64, actual: i64 },

    /// Invalid evidence type for this action
    InvalidType(EvidenceType),

    /// Upload not completed
    UploadIncomplete(EvidenceId),

    /// Evidence expired
    Expired(EvidenceId),

    /// S3 operation failed
    StorageError(String),

    /// Plugin discovery failed
    DiscoveryFailed(String),

    /// Validation failed
    ValidationFailed(String),

    /// Not authorized to access evidence
    AccessDenied(EvidenceId),
}
```

---

## Testing Notes

### Unit Tests

- Evidence type validation
- Storage path generation
- Presigned URL generation
- Retention calculation

### Integration Tests

```
test_upload_evidence_initiate
test_upload_evidence_complete
test_add_link_evidence
test_get_match_evidence
test_get_access_url
test_evidence_expiration
test_plugin_discover_evidence
test_plugin_validate_evidence
test_link_discovered_evidence
test_evidence_access_logging
```

### Edge Case Tests

```
test_large_file_rejection
test_expired_upload_url
test_concurrent_uploads
test_demo_validation_mismatch
test_evidence_after_match_complete
```
