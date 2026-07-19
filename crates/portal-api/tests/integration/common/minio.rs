//! Shared MinIO testcontainers infrastructure.
//!
//! Used by `scanner_e2e` and `evidence_s3` integration tests.

use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// Start a MinIO container and return (container handle, endpoint URL).
pub async fn start_minio() -> (ContainerAsync<GenericImage>, String) {
    let container = GenericImage::new("minio/minio", "latest")
        .with_exposed_port(9000.into())
        .with_env_var("MINIO_ROOT_USER", "minioadmin")
        .with_env_var("MINIO_ROOT_PASSWORD", "minioadmin")
        .with_cmd(vec!["server", "/data"])
        .start()
        .await
        .expect("Failed to start MinIO container");

    let host = container
        .get_host()
        .await
        .expect("Failed to get MinIO host")
        .to_string();
    let port = container
        .get_host_port_ipv4(9000)
        .await
        .expect("Failed to get MinIO port");

    let endpoint = format!("http://{host}:{port}");
    (container, endpoint)
}

/// Build an S3 client pointing at MinIO with static credentials.
pub async fn create_s3_client(endpoint: &str) -> aws_sdk_s3::Client {
    let creds =
        aws_sdk_s3::config::Credentials::new("minioadmin", "minioadmin", None, None, "test");
    let config = aws_config::from_env()
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .endpoint_url(endpoint)
        .credentials_provider(creds)
        .load()
        .await;

    aws_sdk_s3::Client::from_conf(
        aws_sdk_s3::config::Builder::from(&config)
            .force_path_style(true)
            .build(),
    )
}

/// Create an S3 bucket.
pub async fn create_bucket(s3_client: &aws_sdk_s3::Client, bucket: &str) {
    s3_client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("Failed to create S3 bucket");
}

/// Create a bucket and upload a stub file.
pub async fn create_bucket_and_upload(s3_client: &aws_sdk_s3::Client, bucket: &str, key: &str) {
    create_bucket(s3_client, bucket).await;

    s3_client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(aws_sdk_s3::primitives::ByteStream::from_static(b"x"))
        .send()
        .await
        .expect("Failed to upload stub");
}

/// Check if an object exists in the bucket.
pub async fn object_exists(s3_client: &aws_sdk_s3::Client, bucket: &str, key: &str) -> bool {
    match s3_client.head_object().bucket(bucket).key(key).send().await {
        Ok(_) => true,
        Err(_) => false,
    }
}
