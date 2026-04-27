use std::{sync::Arc, time::Duration};

use moka::future::Cache;
use sqlx::Error;

use crate::{
    models::{Product, ProductQuery},
    repositories::ProductRepository,
};

#[derive(Debug, Clone)]
pub enum ServiceError {
    Db,
}

// Fix: was `From<e>` (undefined type `e`), corrected to `From<Error>`.
impl From<Error> for ServiceError {
    fn from(value: Error) -> Self {
        eprintln!("Service error: {}", value);
        Self::Db
    }
}

/// This converts Arc<ServiceError> to ServiceError,
/// to avoid map_err(|e| (*e).clone()) at call sites.
impl From<Arc<ServiceError>> for ServiceError {
    fn from(_value: Arc<ServiceError>) -> Self {
        Self::Db
    }
}

/// Builds a deterministic cache key from a query.
/// Renamed from `build_filters` (which clashed with the SQL builder in repositories.rs)
/// to `cache_key_for_query` to make its purpose unambiguous.
/// Takes a shared reference to avoid an unnecessary clone at call sites.
/// Now includes limit/offset so paginated results are cached independently.
pub fn cache_key_for_query(query: &ProductQuery) -> String {
    let mut key = String::new();

    if let Some(q) = &query.q {
        key += ":name:";
        key += &format!("%{}%", q);
    }
    if let Some(min) = query.min_price {
        key += ":price>:";
        key += &min.to_string();
    }
    if let Some(max) = query.max_price {
        key += ":price<:";
        key += &max.to_string();
    }
    if let Some(cat) = &query.category {
        key += ":category:";
        key += cat;
    }
    if let Some(limit) = query.limit {
        key += ":limit:";
        key += &limit.to_string();
    }
    if let Some(offset) = query.offset {
        key += ":offset:";
        key += &offset.to_string();
    }

    key
}

pub struct Service {
    repo: ProductRepository,
    products_cache: Cache<String, Vec<Product>>,
    categories_cache: Cache<(), Vec<String>>,
}

impl Service {
    pub fn new(repo: ProductRepository) -> Self {
        let products_cache = Cache::builder()
            .time_to_live(Duration::from_mins(5))
            .max_capacity(100)
            .build();
        let categories_cache = Cache::builder()
            .time_to_live(Duration::from_mins(15))
            .max_capacity(100)
            .build();
        Self {
            repo,
            products_cache,
            categories_cache,
        }
    }

    pub async fn list_products(&self, query: ProductQuery) -> Result<Vec<Product>, ServiceError> {
        let key = cache_key_for_query(&query);
        Ok(self
            .products_cache
            .try_get_with(key, self.repo.find_all(query))
            .await?)
    }

    pub async fn get_categories(&self) -> Result<Vec<String>, ServiceError> {
        Ok(self
            .categories_cache
            .try_get_with((), self.repo.get_categories())
            .await?)
    }
}

