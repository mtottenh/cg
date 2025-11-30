//! Validated JSON extractor.

use crate::error::{ApiError, FieldErrorDto};
use axum::extract::rejection::JsonRejection;
use axum::extract::FromRequest;
use axum::http::Request;
use axum::Json;
use serde::de::DeserializeOwned;
use validator::Validate;

/// JSON extractor that validates the request body.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request<axum::body::Body>, state: &S) -> Result<Self, Self::Rejection> {
        // Extract JSON
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|err: JsonRejection| ApiError::bad_request(err.body_text()))?;

        // Validate
        value.validate().map_err(|errors| {
            let field_errors: Vec<FieldErrorDto> = errors
                .field_errors()
                .into_iter()
                .flat_map(|(field, errs)| {
                    errs.iter().map(move |e| FieldErrorDto {
                        field: field.to_string(),
                        message: e
                            .message
                            .clone().map_or_else(|| format!("Validation failed for {field}"), |m| m.to_string()),
                        code: e.code.to_string(),
                    })
                })
                .collect();
            ApiError::validation(field_errors)
        })?;

        Ok(Self(value))
    }
}
