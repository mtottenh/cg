//! Award row structs (`award_templates`, `awards`, `award_results`).

use chrono::{DateTime, Utc};
use portal_core::DomainError;
use portal_domain::entities::award::{
    Award, AwardResult, AwardTemplate, MinQualifier, MinQualifierType,
};
use sqlx::FromRow;
use uuid::Uuid;

/// Row for `award_templates`.
#[derive(Debug, Clone, FromRow)]
pub struct AwardTemplateRow {
    pub id: Uuid,
    pub game_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub stat_key: String,
    pub aggregation: String,
    pub direction: String,
    pub min_qualifier_type: Option<String>,
    pub min_qualifier_value: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// Row for `awards`.
#[derive(Debug, Clone, FromRow)]
pub struct AwardRow {
    pub id: Uuid,
    pub scope_type: String,
    pub scope_id: Uuid,
    pub game_id: Uuid,
    pub template_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub stat_key: String,
    pub aggregation: String,
    pub direction: String,
    pub min_qualifier_type: Option<String>,
    pub min_qualifier_value: Option<i32>,
    pub subject_type: String,
    pub status: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Row for `award_results`.
#[derive(Debug, Clone, FromRow)]
pub struct AwardResultRow {
    pub id: Uuid,
    pub award_id: Uuid,
    pub rank: i32,
    pub player_id: Uuid,
    pub value: f64,
    pub demos_counted: i32,
    pub finalized_at: DateTime<Utc>,
}

/// Reassemble an optional qualifier from its two nullable columns.
fn qualifier_from_columns(
    qualifier_type: Option<&str>,
    value: Option<i32>,
) -> Result<Option<MinQualifier>, DomainError> {
    match (qualifier_type, value) {
        (Some(t), Some(v)) => {
            let qualifier_type: MinQualifierType =
                t.parse().map_err(|e: String| DomainError::internal(e))?;
            Ok(Some(MinQualifier {
                qualifier_type,
                value: v,
            }))
        }
        _ => Ok(None),
    }
}

impl TryFrom<AwardTemplateRow> for AwardTemplate {
    type Error = DomainError;

    fn try_from(row: AwardTemplateRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id.into(),
            game_id: row.game_id.into(),
            key: row.key,
            name: row.name,
            description: row.description,
            icon: row.icon,
            color: row.color,
            stat_key: row.stat_key,
            aggregation: row
                .aggregation
                .parse()
                .map_err(|e: String| DomainError::internal(e))?,
            direction: row
                .direction
                .parse()
                .map_err(|e: String| DomainError::internal(e))?,
            min_qualifier: qualifier_from_columns(
                row.min_qualifier_type.as_deref(),
                row.min_qualifier_value,
            )?,
            created_at: row.created_at,
        })
    }
}

impl TryFrom<AwardRow> for Award {
    type Error = DomainError;

    fn try_from(row: AwardRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id.into(),
            scope_type: row
                .scope_type
                .parse()
                .map_err(|e: String| DomainError::internal(e))?,
            scope_id: row.scope_id,
            game_id: row.game_id.into(),
            template_id: row.template_id.map(Into::into),
            name: row.name,
            description: row.description,
            icon: row.icon,
            color: row.color,
            stat_key: row.stat_key,
            aggregation: row
                .aggregation
                .parse()
                .map_err(|e: String| DomainError::internal(e))?,
            direction: row
                .direction
                .parse()
                .map_err(|e: String| DomainError::internal(e))?,
            min_qualifier: qualifier_from_columns(
                row.min_qualifier_type.as_deref(),
                row.min_qualifier_value,
            )?,
            subject_type: row
                .subject_type
                .parse()
                .map_err(|e: String| DomainError::internal(e))?,
            status: row
                .status
                .parse()
                .map_err(|e: String| DomainError::internal(e))?,
            created_by: row.created_by.into(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl From<AwardResultRow> for AwardResult {
    fn from(row: AwardResultRow) -> Self {
        Self {
            id: row.id.into(),
            award_id: row.award_id.into(),
            rank: row.rank,
            player_id: row.player_id.into(),
            value: row.value,
            demos_counted: row.demos_counted,
            finalized_at: row.finalized_at,
        }
    }
}
