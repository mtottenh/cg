//! S3-compatible storage backend.

use crate::{StorageBackend, StorageError, StoreRequest, StoredFile};
use async_trait::async_trait;
use aws_sdk_s3::Client;
use std::sync::Arc;
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
