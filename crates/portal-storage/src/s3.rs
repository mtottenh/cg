//! S3-compatible storage backend.

use crate::{StorageBackend, StorageError, StoreRequest, StoredFile};
use async_trait::async_trait;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::Client;
use chrono::{DateTime, Utc};
use std::time::Duration;
use tracing::instrument;

/// Configuration for S3 storage.
#[derive(Debug, Clone)]
pub struct S3Config {
    /// S3 bucket name.
    pub bucket: String,
    /// AWS region.
    pub region: String,
    /// Public URL prefix for accessing files.
    pub public_url: String,
    /// Optional custom endpoint (for MinIO, LocalStack, etc.).
    pub endpoint: Option<String>,
}

/// S3-compatible storage backend.
///
/// Works with AWS S3, MinIO, LocalStack, and other S3-compatible services.
#[derive(Debug, Clone)]
pub struct S3Storage {
    client: Client,
    bucket: String,
    public_url: String,
}

impl S3Storage {
    /// Create a new S3 storage backend.
    ///
    /// # Arguments
    ///
    /// * `config` - S3 configuration
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use portal_storage::{S3Storage, S3Config};
    ///
    /// let config = S3Config {
    ///     bucket: "my-bucket".to_string(),
    ///     region: "us-east-1".to_string(),
    ///     public_url: "https://my-bucket.s3.amazonaws.com".to_string(),
    ///     endpoint: None,
    /// };
    ///
    /// let storage = S3Storage::new(config).await;
    /// ```
    pub async fn new(config: S3Config) -> Self {
        let mut aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(config.region));

        if let Some(endpoint) = &config.endpoint {
            aws_config = aws_config.endpoint_url(endpoint);
        }

        let sdk_config = aws_config.load().await;
        let client = Client::new(&sdk_config);

        let mut public_url = config.public_url;
        if public_url.ends_with('/') {
            public_url.pop();
        }

        Self {
            client,
            bucket: config.bucket,
            public_url,
        }
    }

    /// Create from an existing SDK config and bucket.
    #[must_use]
    pub fn from_sdk_config(
        sdk_config: &aws_config::SdkConfig,
        bucket: impl Into<String>,
        public_url: impl Into<String>,
    ) -> Self {
        let client = Client::new(sdk_config);
        let mut public_url = public_url.into();
        if public_url.ends_with('/') {
            public_url.pop();
        }

        Self {
            client,
            bucket: bucket.into(),
            public_url,
        }
    }

    /// Create from a pre-configured S3 client (for MinIO/test with path-style).
    #[must_use]
    pub fn from_client(
        client: Client,
        bucket: impl Into<String>,
        public_url: impl Into<String>,
    ) -> Self {
        let mut public_url = public_url.into();
        if public_url.ends_with('/') {
            public_url.pop();
        }
        Self {
            client,
            bucket: bucket.into(),
            public_url,
        }
    }

    /// Generate a unique storage key.
    fn generate_key(&self, request: &StoreRequest) -> String {
        let id = uuid::Uuid::now_v7();
        let extension = request.filename.rsplit('.').next().unwrap_or("bin");

        match &request.owner_id {
            Some(owner) => format!("{}/{}/{}.{}", request.prefix, owner, id, extension),
            None => format!("{}/{}.{}", request.prefix, id, extension),
        }
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    #[instrument(skip(self, request), fields(bucket = %self.bucket, prefix = %request.prefix))]
    async fn store(&self, request: StoreRequest) -> Result<StoredFile, StorageError> {
        let key = self.generate_key(&request);
        let size = request.data.len() as u64;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(request.data.into())
            .content_type(&request.content_type)
            .send()
            .await
            .map_err(|e| StorageError::S3 {
                message: e.to_string(),
            })?;

        let url = self.public_url(&key);

        tracing::debug!(key = %key, size = size, "File stored to S3");

        Ok(StoredFile {
            url,
            key,
            size,
            content_type: request.content_type,
        })
    }

    #[instrument(skip(self))]
    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::S3 {
                message: e.to_string(),
            })?;

        tracing::debug!(key = %key, "File deleted from S3");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                // Check if it's a NotFound error
                let service_error = e.into_service_error();
                if service_error.is_not_found() {
                    Ok(false)
                } else {
                    Err(StorageError::S3 {
                        message: service_error.to_string(),
                    })
                }
            }
        }
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.public_url, key)
    }
}

// =============================================================================
// EVIDENCE S3 CLIENT
// =============================================================================

/// Metadata about an S3 object.
#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    /// Content length in bytes
    pub content_length: i64,
    /// Content type
    pub content_type: Option<String>,
    /// Last modified timestamp
    pub last_modified: Option<DateTime<Utc>>,
    /// ETag
    pub etag: Option<String>,
}

/// Information about an S3 object (from list operations).
#[derive(Debug, Clone)]
pub struct ObjectInfo {
    /// Object key
    pub key: String,
    /// Size in bytes
    pub size: i64,
    /// Last modified timestamp
    pub last_modified: Option<DateTime<Utc>>,
}

/// Trait for S3 operations needed by the evidence system.
#[async_trait]
pub trait S3EvidenceClient: Send + Sync + 'static {
    /// Generate a presigned PUT URL for uploading.
    async fn presign_put(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        ttl: Duration,
    ) -> Result<String, StorageError>;

    /// Generate a presigned GET URL for downloading.
    async fn presign_get(
        &self,
        bucket: &str,
        key: &str,
        ttl: Duration,
    ) -> Result<String, StorageError>;

    /// Get object metadata without downloading.
    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMetadata, StorageError>;

    /// Delete an object.
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), StorageError>;

    /// List objects with a prefix.
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
    ) -> Result<Vec<ObjectInfo>, StorageError>;

    /// Check if an object exists.
    async fn object_exists(&self, bucket: &str, key: &str) -> Result<bool, StorageError>;
}

#[async_trait]
impl S3EvidenceClient for S3Storage {
    #[instrument(skip(self))]
    async fn presign_put(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        let presign_config = PresigningConfig::expires_in(ttl).map_err(|e| StorageError::S3 {
            message: format!("Failed to create presigning config: {e}"),
        })?;

        let presigned = self
            .client
            .put_object()
            .bucket(bucket)
            .key(key)
            .content_type(content_type)
            .presigned(presign_config)
            .await
            .map_err(|e| StorageError::S3 {
                message: format!("Failed to presign PUT: {e}"),
            })?;

        Ok(presigned.uri().to_string())
    }

    #[instrument(skip(self))]
    async fn presign_get(
        &self,
        bucket: &str,
        key: &str,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        let presign_config = PresigningConfig::expires_in(ttl).map_err(|e| StorageError::S3 {
            message: format!("Failed to create presigning config: {e}"),
        })?;

        let presigned = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .presigned(presign_config)
            .await
            .map_err(|e| StorageError::S3 {
                message: format!("Failed to presign GET: {e}"),
            })?;

        Ok(presigned.uri().to_string())
    }

    #[instrument(skip(self))]
    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMetadata, StorageError> {
        let response = self
            .client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::S3 {
                message: format!("Failed to get object metadata: {e}"),
            })?;

        let last_modified = response
            .last_modified()
            .and_then(|dt| DateTime::from_timestamp(dt.secs(), dt.subsec_nanos()));

        Ok(ObjectMetadata {
            content_length: response.content_length().unwrap_or(0),
            content_type: response.content_type().map(|s| s.to_string()),
            last_modified,
            etag: response.e_tag().map(|s| s.to_string()),
        })
    }

    #[instrument(skip(self))]
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::S3 {
                message: format!("Failed to delete object: {e}"),
            })?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        let mut objects = Vec::new();
        let mut continuation_token = None;

        loop {
            let mut request = self.client.list_objects_v2().bucket(bucket).prefix(prefix);

            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }

            let response = request.send().await.map_err(|e| StorageError::S3 {
                message: format!("Failed to list objects: {e}"),
            })?;

            if let Some(contents) = response.contents {
                for obj in contents {
                    let last_modified = obj
                        .last_modified()
                        .and_then(|dt| DateTime::from_timestamp(dt.secs(), dt.subsec_nanos()));

                    objects.push(ObjectInfo {
                        key: obj.key().unwrap_or("").to_string(),
                        size: obj.size().unwrap_or(0),
                        last_modified,
                    });
                }
            }

            if response.is_truncated.unwrap_or(false) {
                continuation_token = response.next_continuation_token;
            } else {
                break;
            }
        }

        Ok(objects)
    }

    #[instrument(skip(self))]
    async fn object_exists(&self, bucket: &str, key: &str) -> Result<bool, StorageError> {
        match self
            .client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_not_found() {
                    Ok(false)
                } else {
                    Err(StorageError::S3 {
                        message: service_error.to_string(),
                    })
                }
            }
        }
    }
}

/// Get the underlying S3 client for evidence operations.
impl S3Storage {
    /// Get the bucket name.
    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    /// Get the AWS SDK client.
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }
}
