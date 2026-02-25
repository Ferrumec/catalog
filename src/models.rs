use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{config::Permissions, repositories::ProductRepository};

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub price: f64,
    pub category: String,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateProductDto {
    pub name: String,
    pub description: Option<String>,
    pub price: f64,
    pub category: String,
}

#[derive(Debug, Deserialize)]
pub struct ProductQuery {
    pub q: Option<String>,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub category: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Clone)]
pub struct AppState {
    pub repo: ProductRepository,
    pub permissions: Permissions,
}
