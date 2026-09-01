use crate::domain::project::slugify;
use crate::domain::{CatalogError, Project};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProjectRepo {
    pool: PgPool,
}

pub struct NewProject {
    pub name: String,
    pub description: Option<String>,
    pub documentation_url: Option<String>,
    pub owner_id: Uuid,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
}

pub struct MetadataUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub documentation_url: Option<String>,
}

impl ProjectRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// PRJ-001/PRJ-016: create a project with initial metadata. Not yet
    /// published (PRJ-002: publication happens once a version passes
    /// validation — see `VersionRepo::mark_validated`).
    pub async fn create(&self, new: NewProject) -> Result<Project, CatalogError> {
        if new.name.trim().chars().count() < 3 {
            return Err(CatalogError::BadRequest(
                "project name must be at least 3 characters".into(),
            ));
        }
        let base_slug = slugify(&new.name);
        let mut tx = self.pool.begin().await?;

        // Slugs are unique; retry with a numeric suffix on collision
        // rather than making the caller pick one. Bounded so a pathological
        // run of collisions can't loop forever.
        let mut slug = base_slug.clone();
        let mut attempt = 0u32;
        let project = loop {
            let result = sqlx::query_as::<_, Project>(
                r#"
                INSERT INTO projects (name, slug, description, documentation_url, owner_id)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id, name, slug, description, documentation_url, owner_id,
                          current_version_id, is_published, created_at, updated_at,
                          deleted_at, popularity_score
                "#,
            )
            .bind(&new.name)
            .bind(&slug)
            .bind(&new.description)
            .bind(&new.documentation_url)
            .bind(new.owner_id)
            .fetch_one(&mut *tx)
            .await;

            match result {
                Ok(p) => break p,
                Err(sqlx::Error::Database(e)) if e.is_unique_violation() && attempt < 20 => {
                    attempt += 1;
                    slug = format!("{base_slug}-{attempt}");
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        };

        for tag in &new.tags {
            sqlx::query(
                "INSERT INTO project_tags (project_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(project.id)
            .bind(tag.trim().to_lowercase())
            .execute(&mut *tx)
            .await?;
        }
        for category in &new.categories {
            sqlx::query(
                "INSERT INTO project_categories (project_id, category) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(project.id)
            .bind(category.trim().to_lowercase())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(project)
    }

    pub async fn get(&self, id: Uuid) -> Result<Project, CatalogError> {
        sqlx::query_as::<_, Project>(
            r#"
            SELECT id, name, slug, description, documentation_url, owner_id,
                   current_version_id, is_published, created_at, updated_at,
                   deleted_at, popularity_score
            FROM projects WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(CatalogError::ProjectNotFound)
    }

    pub async fn get_by_slug(&self, slug: &str) -> Result<Project, CatalogError> {
        sqlx::query_as::<_, Project>(
            r#"
            SELECT id, name, slug, description, documentation_url, owner_id,
                   current_version_id, is_published, created_at, updated_at,
                   deleted_at, popularity_score
            FROM projects WHERE slug = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(CatalogError::ProjectNotFound)
    }

    pub async fn tags_for(&self, project_id: Uuid) -> Result<Vec<String>, CatalogError> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT tag FROM project_tags WHERE project_id = $1 ORDER BY tag")
                .bind(project_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(t,)| t).collect())
    }

    pub async fn categories_for(&self, project_id: Uuid) -> Result<Vec<String>, CatalogError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT category FROM project_categories WHERE project_id = $1 ORDER BY category",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(c,)| c).collect())
    }

    /// PRJ-017/PRJ-018: metadata can be updated independent of versions;
    /// every change is recorded to `project_metadata_history`.
    pub async fn update_metadata(
        &self,
        project_id: Uuid,
        changed_by: Uuid,
        update: MetadataUpdate,
    ) -> Result<Project, CatalogError> {
        let mut tx = self.pool.begin().await?;

        let before = sqlx::query_as::<_, Project>(
            r#"SELECT id, name, slug, description, documentation_url, owner_id,
                      current_version_id, is_published, created_at, updated_at,
                      deleted_at, popularity_score
               FROM projects WHERE id = $1 AND deleted_at IS NULL FOR UPDATE"#,
        )
        .bind(project_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(CatalogError::ProjectNotFound)?;

        let name = update.name.unwrap_or_else(|| before.name.clone());
        let description = update.description.or_else(|| before.description.clone());
        let documentation_url = update
            .documentation_url
            .or_else(|| before.documentation_url.clone());

        let after = sqlx::query_as::<_, Project>(
            r#"
            UPDATE projects
            SET name = $1, description = $2, documentation_url = $3, updated_at = NOW()
            WHERE id = $4
            RETURNING id, name, slug, description, documentation_url, owner_id,
                      current_version_id, is_published, created_at, updated_at,
                      deleted_at, popularity_score
            "#,
        )
        .bind(&name)
        .bind(&description)
        .bind(&documentation_url)
        .bind(project_id)
        .fetch_one(&mut *tx)
        .await?;

        let previous_value = serde_json::json!({
            "name": before.name,
            "description": before.description,
            "documentation_url": before.documentation_url,
        });
        let new_value = serde_json::json!({
            "name": after.name,
            "description": after.description,
            "documentation_url": after.documentation_url,
        });
        sqlx::query(
            r#"INSERT INTO project_metadata_history (project_id, changed_by, previous_value, new_value)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(project_id)
        .bind(changed_by)
        .bind(previous_value)
        .bind(new_value)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(after)
    }

    pub async fn replace_tags(
        &self,
        project_id: Uuid,
        tags: &[String],
    ) -> Result<(), CatalogError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM project_tags WHERE project_id = $1")
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
        for tag in tags {
            sqlx::query(
                "INSERT INTO project_tags (project_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(project_id)
            .bind(tag.trim().to_lowercase())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn replace_categories(
        &self,
        project_id: Uuid,
        categories: &[String],
    ) -> Result<(), CatalogError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM project_categories WHERE project_id = $1")
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
        for category in categories {
            sqlx::query(
                "INSERT INTO project_categories (project_id, category) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(project_id)
            .bind(category.trim().to_lowercase())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// PRJ-005: soft delete so version history/artifacts (PRJ-023-style
    /// retention) and any in-flight deployments referencing the project
    /// id aren't orphaned by a hard delete.
    pub async fn soft_delete(&self, project_id: Uuid, owner_id: Uuid) -> Result<(), CatalogError> {
        let project = self.get(project_id).await?;
        if project.owner_id != owner_id {
            return Err(CatalogError::NotOwner);
        }
        sqlx::query("UPDATE projects SET deleted_at = NOW(), is_published = FALSE WHERE id = $1")
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// PRJ-006: set the "current" version pointer. Does not affect
    /// clients pinned to an earlier version — this only changes what
    /// *new* deployments/tests resolve to when no version is specified.
    pub async fn set_current_version(
        &self,
        project_id: Uuid,
        version_id: Uuid,
    ) -> Result<(), CatalogError> {
        sqlx::query(
            "UPDATE projects SET current_version_id = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(version_id)
        .bind(project_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn publish(&self, project_id: Uuid) -> Result<(), CatalogError> {
        sqlx::query("UPDATE projects SET is_published = TRUE, updated_at = NOW() WHERE id = $1")
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// DSC-009: popularity is tracked here; what feeds it (downloads,
    /// deployments, subscriptions) arrives asynchronously as domain
    /// events from Billing/Execution over the NATS bus rather than a
    /// synchronous call, matching the dependency viewpoint in SDD §3.2.
    pub async fn bump_popularity(&self, project_id: Uuid, delta: f64) -> Result<(), CatalogError> {
        sqlx::query("UPDATE projects SET popularity_score = popularity_score + $1 WHERE id = $2")
            .bind(delta)
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
