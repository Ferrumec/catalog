use async_trait::async_trait;
use ferrumec::Permission;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{CatalogModule, config::Permissions, repositories::ProductRepository};

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

#[derive(Debug, Deserialize, Clone)]
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

#[async_trait]
pub trait OnCreateHandler: Send + Sync {
    async fn handle(&self, dto: CreateProductDto) -> bool;
}

pub struct DefaultOnCreate;

#[async_trait]
impl OnCreateHandler for DefaultOnCreate {
    async fn handle(&self, dto: CreateProductDto) -> bool {
        println!("Default on create handler: {}", dto.name);
        true
    }
}
pub struct AppState {
    pub repo: ProductRepository,
    pub permissions: Permissions,
    pub on_create_product: Box<dyn OnCreateHandler>,
}

pub struct Config {
    pub repo: Option<ProductRepository>,
    pub permissions: Option<Permissions>,
    pub on_create_product: Option<Box<dyn OnCreateHandler>>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            repo: None,
            permissions: None,
            on_create_product: None,
        }
    }
    pub fn with_on_create(mut self, on_create: Box<dyn OnCreateHandler>) -> Self {
        self.on_create_product = Some(on_create);
        self
    }
    pub fn with_perms(mut self, perms: Vec<Permission>) -> Self {
        self.permissions = Some(CatalogModule::set_permissions(perms));
        self
    }
}
