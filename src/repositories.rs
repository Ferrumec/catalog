use crate::{
    models::{CreateProductDto, Product, ProductQuery},
    service::ServiceError,
};
use chrono::Utc;
use sqlx::{Error, QueryBuilder, Row, Sqlite, SqlitePool, query_as, query_scalar};
use uuid::Uuid;

#[derive(Clone)]
pub struct ProductRepository {
    pool: SqlitePool,
}

fn generate_slug(name: &str) -> String {
    name.to_lowercase()
        .replace(" ", "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "")
}

pub fn build_filters(mut qb: QueryBuilder<Sqlite>, query: ProductQuery) -> QueryBuilder<Sqlite> {
    if let Some(q) = query.q {
        let pattern = format!("%{}%", q);
        qb.push(" AND (name LIKE ");
        qb.push_bind(pattern.clone());
        qb.push(" OR description LIKE ");
        qb.push_bind(pattern);
        qb.push(")");
    }

    if let Some(min) = query.min_price {
        qb.push(" AND price >= ");
        qb.push_bind(min);
    }

    if let Some(max) = query.max_price {
        qb.push(" AND price <= ");
        qb.push_bind(max);
    }

    if let Some(cat) = query.category {
        qb.push(" AND category = ");
        qb.push_bind(cat);
    }

    qb.push(" ORDER BY created_at DESC");

    let limit = query.limit.unwrap_or(20);
    let offset = query.offset.unwrap_or(0);

    qb.push(" LIMIT ");
    qb.push_bind(limit);
    qb.push(" OFFSET ");
    qb.push_bind(offset);

    qb
}

impl ProductRepository {
    pub async fn new(pool: SqlitePool) -> Result<Self, Error> {
        ProductRepository::create_table(&pool).await?;
        Ok(Self { pool })
    }
    pub async fn create(&self, dto: CreateProductDto) -> Result<Product, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().timestamp_micros();
        let slug = generate_slug(&dto.name);
        sqlx::query(
            "INSERT INTO products (id, name, slug, description, price, category, created_at, sku)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(id.clone())
        .bind(&dto.name)
        .bind(slug.clone())
        .bind(&dto.description)
        .bind(dto.price)
        .bind(dto.category.clone())
        .bind(created_at)
        .bind(dto.sku.clone())
        .execute(&self.pool)
        .await?;

        Ok(Product {
            id,
            name: dto.name,
            slug: slug,
            sku: dto.sku,
            description: dto.description,
            price: dto.price,
            category: dto.category,
            created_at,
        })
    }

    pub async fn find_by_id(&self, id: String) -> Result<Option<Product>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, name, slug, description, price, category, created_at, sku
             FROM products
             WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(Product {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            price: row.try_get("price")?,
            slug: row.try_get("slug")?,
            sku: row.try_get("sku")?,
            category: row.try_get("category")?,
            created_at: row.try_get("created_at")?,
        }))
    }

    pub async fn find_all(&self, query: ProductQuery) -> Result<Vec<Product>, ServiceError> {
        let mut qb = QueryBuilder::new(
            "SELECT id, name,slug, sku, description, price, category, created_at FROM products WHERE 1=1",
        );

        qb = build_filters(qb, query);

        let products: Vec<Product> = qb
            .build_query_as::<Product>() // <--- automatic mapping
            .fetch_all(&self.pool)
            .await?;

        Ok(products)
    }

    pub async fn find_by_slug(&self, slug: &str) -> Result<Option<Product>, Error> {
        let row = sqlx::query(
            "SELECT id, name, description, price, category, slug, created_at
         FROM products
         WHERE slug = ?1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(Product {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            price: row.try_get("price")?,
            sku: row.try_get("sku")?,
            category: row.try_get("category")?,
            slug: row.try_get("slug")?,
            created_at: row.try_get("created_at")?,
        }))
    }

    pub async fn get_categories(&self) -> Result<Vec<String>, ServiceError> {
        Ok(query_scalar::<_, String>("SELECT category FROM products")
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn create_table(pool: &SqlitePool) -> Result<(), Error> {
        sqlx::query(
            r#"
        CREATE TABLE IF NOT EXISTS products (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
        slug TEXT NOT NULL,
            description TEXT,
            price REAL NOT NULL,
            category TEXT NOT NULL,
            sku TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_products_slug ON products(slug);
CREATE INDEX IF NOT EXISTS idx_products_category ON products(category);
CREATE INDEX IF NOT EXISTS idx_products_price ON products(price);
CREATE INDEX IF NOT EXISTS idx_products_created_at ON products(created_at);
        "#,
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}
