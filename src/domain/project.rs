use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub documentation_url: Option<String>,
    pub owner_id: Uuid,
    pub current_version_id: Option<Uuid>,
    pub is_published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub popularity_score: f64,
}

/// Public representation returned from the API (PRJ-016, DSC-002).
#[derive(Serialize)]
pub struct ProjectDto {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub documentation_url: Option<String>,
    pub owner_id: Uuid,
    pub current_version_id: Option<Uuid>,
    pub is_published: bool,
    pub popularity_score: f64,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectDto {
    pub fn from_project(p: Project, tags: Vec<String>, categories: Vec<String>) -> Self {
        Self {
            id: p.id,
            name: p.name,
            slug: p.slug,
            description: p.description,
            documentation_url: p.documentation_url,
            owner_id: p.owner_id,
            current_version_id: p.current_version_id,
            is_published: p.is_published,
            popularity_score: p.popularity_score,
            tags,
            categories,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

/// Turn a project name into a URL-safe, unique-per-attempt slug.
/// Collisions (PRJ slugs are UNIQUE) are handled by the caller retrying
/// with a numeric suffix — see `ProjectRepo::create`.
pub fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_dash = false;
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("project");
    }
    slug
}
