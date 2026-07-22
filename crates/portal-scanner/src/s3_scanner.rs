//! S3 bucket scanner with pagination.

use anyhow::{Context, Result};
use tracing::debug;

/// A discovered S3 object.
#[derive(Debug, Clone)]
pub struct S3Object {
    /// S3 key (path within the bucket).
    pub key: String,
    /// File size in bytes.
    pub size: i64,
}

/// Scan an S3 bucket for demo files (.dem extension).
///
/// Uses pagination to handle large buckets.
pub async fn list_demo_files(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    prefix: &str,
) -> Result<Vec<S3Object>> {
    let mut objects = Vec::new();
    let mut continuation_token: Option<String> = None;

    loop {
        let mut req = client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .max_keys(1000);

        if let Some(token) = &continuation_token {
            req = req.continuation_token(token);
        }

        let resp = req.send().await.context("S3 list_objects_v2 failed")?;

        for obj in resp.contents() {
            let key = obj.key().unwrap_or_default();
            // Only include demo files (.dem.bz2 or .dem)
            if key.ends_with(".dem.bz2")
                || std::path::Path::new(key)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dem"))
            {
                objects.push(S3Object {
                    key: key.to_string(),
                    size: obj.size().unwrap_or(0),
                });
            }
        }

        if resp.is_truncated() == Some(true) {
            continuation_token = resp.next_continuation_token().map(String::from);
        } else {
            break;
        }
    }

    debug!(
        count = objects.len(),
        bucket, prefix, "Listed demo files from S3"
    );
    Ok(objects)
}
