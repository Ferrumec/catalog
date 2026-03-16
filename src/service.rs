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

impl From<Error> for ServiceError {
    fn from(value: Error) -> Self {
        // Log the error here since the message is not kept
        eprintln!("Service error: {}", value);
        Self::Db
    }
}

/// This converts Arc<ServiceError> to ServiceError.
/// just to do away with map_err(|e|(*e).clone()).
/// We can as well convert Arc<sqlx::Error> to ServiceError but that will swallow sqlx error details.
impl From<Arc<ServiceError>> for ServiceError {
    fn from(_value: Arc<ServiceError>) -> Self {
        Self::Db
    }
}

pub fn build_filters(query: ProductQuery) -> String {
    let mut qb: String = String::new();
    if let Some(q) = query.q {
        let pattern = format!("%{}%", q);
        qb += ":name:";
        qb += &pattern;
    }

    if let Some(min) = query.min_price {
        qb += ":price>:";
        qb += &min.to_string();
    }

    if let Some(max) = query.max_price {
        qb += ":price<:";
        qb += &max.to_string();
    }

    if let Some(cat) = query.category {
        qb += ":category:";
        qb += &cat.to_string();
    }
    qb
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
        let key = build_filters(query.clone());
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
