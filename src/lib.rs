mod config;
mod models;
mod repositories;
mod routes;

pub use config::CatalogModule;
pub use models::{Config, CreateProductDto, OnCreateHandler};
