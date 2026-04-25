use std::{str::FromStr, sync::Arc, time::Duration};

use event_stream::EventStream;
use ferrumec::{CreateItem, OnCreateHandler, Permission};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tera::Tera;

use crate::{
    CatalogModule, config::Permissions, repositories::ProductRepository, service::Service,
};

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

#[derive(Debug, Deserialize, Clone)]
pub struct UpdateProductDto {
    pub name: Option<String>,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub category: Option<String>,
    pub qty: Option<u32>,
    pub sku: Option<String>,
}

#[derive(Clone, Deserialize)]
pub struct ProductQuery {
    pub q: Option<String>,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub category: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Clone, Deserialize)]
pub struct SafeProductQuery {
    pub q: Option<String>,
    pub min_price: Option<String>,
    pub max_price: Option<String>,
    pub category: Option<String>,
    pub limit: Option<String>,
    pub offset: Option<String>,
}

fn to_opt_t<T>(string: Option<String>) -> Option<T>
where
    T: FromStr,
{
    match string {
        Some(t) => {
            if t.is_empty() {
                None
            } else {
                match T::from_str(&t) {
                    Ok(t) => Some(t),
                    Err(_) => {
                        eprintln!("Error parsing value defaulting to None");
                        None
                    }
                }
            }
        }
        None => None,
    }
}

impl From<SafeProductQuery> for ProductQuery {
    fn from(value: SafeProductQuery) -> Self {
        Self {
            q: to_opt_t(value.q),
            min_price: to_opt_t(value.min_price),
            max_price: to_opt_t(value.max_price),
            category: to_opt_t(value.category),
            limit: to_opt_t(value.limit),
            offset: to_opt_t(value.offset),
        }
    }
}

pub struct Caches {
    pub catalog_page: Cache<String, String>,
}

impl Caches {
    pub fn new() -> Self {
        let catalog_page = Cache::builder()
            .time_to_live(Duration::from_secs(60))
            .max_capacity(100)
            .build();
        Self { catalog_page }
    }
}
pub struct AppState {
    pub caches: Caches,
    pub tera: Tera,
    pub repo: ProductRepository,
    pub service: Service,
    pub permissions: Permissions,
    pub es: Arc<dyn EventStream>,
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
