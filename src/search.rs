//! Project discovery (DSC-001–009). Search runs against Catalog's own
//! Postgres instance via `tsvector`/GIN (DES-003) — no separate search
//! cluster, per SDD §9.

use crate::domain::{CatalogError, Project};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    Relevance,
    Popularity,
    Recency,
}

#[derive(Debug, Deserialize)]
pub struct DiscoveryQuery {
    /// DSC-003: search by name (and description, via the same FTS
    /// index — SDD §7.2 weights name higher, see the migration trigger).
    pub q: Option<String>,
    /// DSC-004: filter by tags (all must match). Comma-separated
    /// (`?tags=web,cli`) rather than repeated query keys — actix-web's
    /// query extractor goes through `serde_urlencoded`, which doesn't
    /// collect repeated keys into a `Vec`.
    pub tags: Option<String>,
    /// DSC-004: filter by categories (all must match). Same
    /// comma-separated convention as `tags`.
    pub categories: Option<String>,
    /// DSC-005: filter by author.
    pub author_id: Option<Uuid>,
    /// DSC-006. Defaults to relevance when `q` is set, else recency —
    /// "relevance" is meaningless without a query term.
    pub sort: Option<SortBy>,
    /// DSC-008.
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    20
}

#[derive(Serialize)]
pub struct DiscoveryPage {
    pub items: Vec<crate::domain::ProjectDto>,
    pub page: u32,
    pub page_size: u32,
    pub total: i64,
}

fn split_csv(s: Option<&str>) -> Option<Vec<String>> {
    let s = s?;
    let items: Vec<String> = s
        .split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

pub async fn discover(pool: &PgPool, query: DiscoveryQuery) -> Result<DiscoveryPage, CatalogError> {
    let page = query.page.max(1);
    let page_size = query.page_size.clamp(1, 100);
    let offset = ((page - 1) * page_size) as i64;
    let limit = page_size as i64;

    let sort = query.sort.unwrap_or(if query.q.is_some() {
        SortBy::Relevance
    } else {
        SortBy::Recency
    });

    // DSC-001/DSC-004/DSC-005: only published, non-deleted projects are
    // discoverable; a project a developer hasn't finished validating a
    // version for (PRJ-002) shouldn't show up in browse/search.
    let ts_query = query
        .q
        .as_ref()
        .filter(|q| !q.trim().is_empty())
        .map(|q| q.trim().to_string());

    let order_by = match (sort, ts_query.is_some()) {
        (SortBy::Relevance, true) => "rank DESC, popularity_score DESC",
        (SortBy::Relevance, false) => "popularity_score DESC, created_at DESC", // no query text to rank
        (SortBy::Popularity, _) => "popularity_score DESC, created_at DESC",
        (SortBy::Recency, _) => "created_at DESC",
    };

    // Built with a fixed set of possible shapes (query / no query) and
    // bound parameters throughout — `order_by`/`rank_select` above are
    // chosen from a hardcoded Rust match/branch, never interpolated
    // from user input, so this stays injection-safe despite the
    // interpolation. sqlx 0.9's `SqlSafeStr` guard only accepts
    // `&'static str` by default (it can't know that at compile time for
    // a runtime-built `String`), so the audited string is wrapped in
    // `AssertSqlSafe` to say so explicitly — every value that reaches
    // the database itself still goes through `.bind(..)`, never through
    // this string.
    let sql = format!(
        r#"
        SELECT p.id, p.name, p.slug, p.description, p.documentation_url, p.owner_id,
               p.current_version_id, p.is_published, p.created_at, p.updated_at,
               p.deleted_at, p.popularity_score,
               COUNT(*) OVER() AS total_count
        FROM projects p
        WHERE p.deleted_at IS NULL
          AND p.is_published = TRUE
          AND ($1::uuid IS NULL OR p.owner_id = $1)
          AND ($2::text[] IS NULL OR $2 <@ (
                SELECT array_agg(tag) FROM project_tags WHERE project_id = p.id))
          AND ($3::text[] IS NULL OR $3 <@ (
                SELECT array_agg(category) FROM project_categories WHERE project_id = p.id))
          AND ($4::text IS NULL OR p.search_vector @@ websearch_to_tsquery('english', $4))
        {rank_select}
        ORDER BY {order_by}
        LIMIT $5 OFFSET $6
        "#,
        rank_select = if ts_query.is_some() {
            ", ts_rank(p.search_vector, websearch_to_tsquery('english', $4)) AS rank"
        } else {
            ""
        },
    );

    let tags = split_csv(query.tags.as_deref());
    let categories = split_csv(query.categories.as_deref());

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(query.author_id)
        .bind(tags)
        .bind(categories)
        .bind(&ts_query)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

    use sqlx::Row;
    let total = rows
        .first()
        .map(|r| r.get::<i64, _>("total_count"))
        .unwrap_or(0);

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let project = Project {
            id: row.get("id"),
            name: row.get("name"),
            slug: row.get("slug"),
            description: row.get("description"),
            documentation_url: row.get("documentation_url"),
            owner_id: row.get("owner_id"),
            current_version_id: row.get("current_version_id"),
            is_published: row.get("is_published"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            deleted_at: row.get("deleted_at"),
            popularity_score: row.get("popularity_score"),
        };
        // DSC-002: tags/categories are part of "project information" —
        // fetched per row here rather than joined/aggregated into the
        // page query above, trading a few extra round trips (bounded by
        // page_size, ≤100) for a search query simple enough to keep the
        // GIN/rank plan predictable.
        let tags = sqlx::query_scalar::<_, String>(
            "SELECT tag FROM project_tags WHERE project_id = $1 ORDER BY tag",
        )
        .bind(project.id)
        .fetch_all(pool)
        .await?;
        let categories = sqlx::query_scalar::<_, String>(
            "SELECT category FROM project_categories WHERE project_id = $1 ORDER BY category",
        )
        .bind(project.id)
        .fetch_all(pool)
        .await?;
        items.push(crate::domain::ProjectDto::from_project(
            project, tags, categories,
        ));
    }

    Ok(DiscoveryPage {
        items,
        page,
        page_size,
        total,
    })
}
