use async_trait::async_trait;
use ferrumec::{CreateItem, OnCreateHandler, Permission};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{CatalogModule, config::Permissions, repositories::ProductRepository};

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub price: f64,
    pub sku: String,
    pub category: String,
    pub created_at: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CreateProductDto {
    pub name: String,
    pub description: Option<String>,
    pub price: f64,
    pub category: String,
    pub qty: u32,
    pub sku: String,
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

pub struct DefaultOnCreate;

#[async_trait]
impl OnCreateHandler for DefaultOnCreate {
    type Dto = CreateItem;
    async fn handle(&self, dto: CreateItem) -> bool {
        println!("Default on create handler: {}", dto.id);
        true
    }
}
pub struct AppState {
    pub repo: ProductRepository,
    pub permissions: Permissions,
    pub on_create_product: Box<dyn OnCreateHandler<Dto = CreateItem>>,
}

pub struct Config {
    pub repo: Option<ProductRepository>,
    pub permissions: Option<Permissions>,
    pub on_create_product: Option<Box<dyn OnCreateHandler<Dto = CreateItem>>>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            repo: None,
            permissions: None,
            on_create_product: None,
        }
    }
    pub fn with_on_create(mut self, on_create: Box<dyn OnCreateHandler<Dto = CreateItem>>) -> Self {
        self.on_create_product = Some(on_create);
        self
    }
    pub fn with_perms(mut self, perms: Vec<Permission>) -> Self {
        self.permissions = Some(CatalogModule::set_permissions(perms));
        self
    }
}
