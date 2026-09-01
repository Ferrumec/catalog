use crate::domain::version::ValidationStatus;
use crate::domain::{CatalogError, Version};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct VersionRepo {
    pool: PgPool,
}

pub struct NewVersion {
    pub project_id: Uuid,
    pub version_tag: String,
    pub artifact_url: String,
    pub artifact_size: i64,
    pub checksum: String,
    pub developer_id: Uuid,
}

impl VersionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// PRJ-001/PRJ-008: each upload is a new, immutable version with a
    /// unique id. Starts `pending` — validation (PRJ-002) runs after
    /// this row exists so failure reports (PRJ-015) have somewhere to
    /// attach.
    pub async fn create(&self, new: NewVersion) -> Result<Version, CatalogError> {
        sqlx::query_as::<_, Version>(
            r#"
            INSERT INTO versions (project_id, version_tag, artifact_url, artifact_size, checksum, developer_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, project_id, version_tag, artifact_url, artifact_size, checksum,
                      is_yanked, yank_reason, yanked_at, yanked_by, unyanked_at, unyanked_by,
                      validation_status, validation_details, developer_id, created_at, validated_at
            "#,
        )
        .bind(new.project_id)
        .bind(&new.version_tag)
        .bind(&new.artifact_url)
        .bind(new.artifact_size)
        .bind(&new.checksum)
        .bind(new.developer_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => CatalogError::VersionTagTaken,
            _ => CatalogError::Database(e),
        })
    }

    pub async fn get(&self, id: Uuid) -> Result<Version, CatalogError> {
        sqlx::query_as::<_, Version>(
            r#"
            SELECT id, project_id, version_tag, artifact_url, artifact_size, checksum,
                   is_yanked, yank_reason, yanked_at, yanked_by, unyanked_at, unyanked_by,
                   validation_status, validation_details, developer_id, created_at, validated_at
            FROM versions WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(CatalogError::VersionNotFound)
    }

    pub async fn list_for_project(&self, project_id: Uuid) -> Result<Vec<Version>, CatalogError> {
        Ok(sqlx::query_as::<_, Version>(
            r#"
            SELECT id, project_id, version_tag, artifact_url, artifact_size, checksum,
                   is_yanked, yank_reason, yanked_at, yanked_by, unyanked_at, unyanked_by,
                   validation_status, validation_details, developer_id, created_at, validated_at
            FROM versions WHERE project_id = $1 ORDER BY created_at DESC
            "#,
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// PRJ-002/PRJ-014/PRJ-015: record the validation pipeline's
    /// verdict. `details` carries a per-stage report so developers get
    /// an actionable failure message, not just pass/fail.
    pub async fn mark_validated(
        &self,
        version_id: Uuid,
        status: ValidationStatus,
        details: serde_json::Value,
    ) -> Result<(), CatalogError> {
        sqlx::query(
            r#"UPDATE versions
               SET validation_status = $1, validation_details = $2, validated_at = NOW()
               WHERE id = $3"#,
        )
        .bind(status.as_str())
        .bind(details)
        .bind(version_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// PRJ-019/PRJ-021: remove from new-deployment eligibility. Existing
    /// running deployments are untouched here — nothing in this table
    /// tracks live instances (that's Execution's `execution_instances`,
    /// a different service/database entirely), so there's nothing to
    /// cascade to (PRJ-020 is satisfied by *absence* of any such link).
    pub async fn yank(
        &self,
        version_id: Uuid,
        owner_id: Uuid,
        yanked_by: Uuid,
        reason: Option<String>,
    ) -> Result<Version, CatalogError> {
        self.assert_owns(version_id, owner_id).await?;
        let version = self.get(version_id).await?;
        if version.is_yanked {
            return Err(CatalogError::AlreadyYanked);
        }
        sqlx::query(
            r#"UPDATE versions
               SET is_yanked = TRUE, yank_reason = $1, yanked_at = NOW(), yanked_by = $2
               WHERE id = $3"#,
        )
        .bind(&reason)
        .bind(yanked_by)
        .bind(version_id)
        .execute(&self.pool)
        .await?;
        self.get(version_id).await
    }

    /// PRJ-022: reverse a yank, restoring deployment eligibility.
    pub async fn unyank(
        &self,
        version_id: Uuid,
        owner_id: Uuid,
        unyanked_by: Uuid,
    ) -> Result<Version, CatalogError> {
        self.assert_owns(version_id, owner_id).await?;
        let version = self.get(version_id).await?;
        if !version.is_yanked {
            return Err(CatalogError::NotYanked);
        }
        sqlx::query(
            r#"UPDATE versions
               SET is_yanked = FALSE, unyanked_at = NOW(), unyanked_by = $1
               WHERE id = $2"#,
        )
        .bind(unyanked_by)
        .bind(version_id)
        .execute(&self.pool)
        .await?;
        self.get(version_id).await
    }

    async fn assert_owns(&self, version_id: Uuid, owner_id: Uuid) -> Result<(), CatalogError> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"SELECT p.owner_id FROM versions v JOIN projects p ON p.id = v.project_id
               WHERE v.id = $1 AND p.deleted_at IS NULL"#,
        )
        .bind(version_id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some((owner,)) if owner == owner_id => Ok(()),
            Some(_) => Err(CatalogError::NotOwner),
            None => Err(CatalogError::VersionNotFound),
        }
    }
}
