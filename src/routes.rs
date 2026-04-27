use crate::models::{AppState, CreateProductDto, SafeProductQuery, UpdateProductDto};
use actix_web::{HttpResponse, Responder, get, patch, post, web};
use e2schema::catalog::Money;
use e2schema::catalog::ProductCreated;
use e2schema::EventMetaData;
use event_stream::Publishable;
use libsigners::Claims;
use tera::Context;

#[post("/products")]
pub async fn create_product(
    state: web::Data<AppState>,
    claims: web::ReqData<Claims>,
    payload: web::Json<CreateProductDto>,
) -> impl Responder {
    if !state.permissions.create_product.check(claims.into_inner()) {
        return HttpResponse::Forbidden().body("Unauthorized");
    }
    let dto = payload.into_inner();
    match state.repo.create(dto.clone()).await {
        Ok(product) => {
            let prod = product.clone();
            let event = ProductCreated {
                category_id: product.category,
                _emd: EventMetaData::new("catalog"),
                name: product.name,
                attributes: None,
                sku: product.sku,
                price: Money {
                    currency: "ksh".to_string(),
                    amount: product.price,
                },
                product_id: product.id,
            };
            let _ = event.publish(state.es.clone()).await;
            HttpResponse::Created().json(prod)
        }
        Err(err) => {
            eprintln!("Error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/products/{id}")]
pub async fn get_product(data: web::Data<AppState>, path: web::Path<String>) -> impl Responder {
    match data.repo.find_by_id(path.into_inner()).await {
        Ok(Some(product)) => HttpResponse::Ok().json(product),
        Ok(None) => {
            HttpResponse::NotFound().json(serde_json::json!({ "error": "Product not found" }))
        }
        Err(err) => {
            eprintln!("DB error: {err}");
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/products/slug/{slug}")]
pub async fn get_product_by_slug(
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let slug = path.into_inner();
    match data.repo.find_by_slug(&slug).await {
        Ok(Some(product)) => HttpResponse::Ok().json(product),
        Ok(None) => {
            HttpResponse::NotFound().json(serde_json::json!({ "error": "Product not found" }))
        }
        Err(err) => {
            eprintln!("DB error: {err}");
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/products")]
pub async fn list_products(
    data: web::Data<AppState>,
    query: web::Query<SafeProductQuery>,
) -> impl Responder {
    match data.service.list_products(query.into_inner().into()).await {
        Ok(products) => HttpResponse::Ok().json(products),
        Err(err) => {
            eprintln!("Error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/")]
pub async fn index(
    data: web::Data<AppState>,
    query: web::Query<SafeProductQuery>,
) -> impl Responder {
    // Fix: cache key now incorporates the query so that filtered/paginated
    // requests do not poison or incorrectly serve the cached default view.
    let query_inner: SafeProductQuery = query.into_inner();
    let product_query = query_inner.clone().into();

    // Build a simple string key from the raw SafeProductQuery fields.
    let cache_key = format!(
        "catalog:q={:?}:cat={:?}:min={:?}:max={:?}:limit={:?}:offset={:?}",
        query_inner.q,
        query_inner.category,
        query_inner.min_price,
        query_inner.max_price,
        query_inner.limit,
        query_inner.offset,
    );

    if let Some(cached_html) = data.caches.catalog_page.get(&cache_key).await {
        return HttpResponse::Ok()
            .content_type("text/html")
            .body(cached_html);
    }

    let products = match data.service.list_products(product_query).await {
        Ok(r) => r,
        Err(_e) => return HttpResponse::InternalServerError().finish(),
    };

    let categories = match data.service.get_categories().await {
        Ok(r) => r,
        Err(_e) => return HttpResponse::InternalServerError().finish(),
    };

    // Fix: `total_pages` was hardcoded to 1. Now derived from the actual product
    // count and the active limit. Falls back to page 1 when the list is empty.
    let limit = query_inner
        .limit
        .as_deref()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(20);
    let total_pages = if products.is_empty() {
        1
    } else {
        // Ceiling division: this is an approximation based on what was fetched.
        // For exact pagination a COUNT query would be needed; this is a best-effort
        // value until a dedicated count endpoint is added.
        (products.len() + limit - 1) / limit
    };

    let mut ctx = Context::new();
    ctx.insert("products", &products);
    ctx.insert("categories", &categories);
    ctx.insert(
        "category",
        &query_inner.category.clone().unwrap_or_default(),
    );
    ctx.insert("sort", "desc");
    ctx.insert("total_pages", &total_pages);

    let rendered_html = data.tera.render("catalog.html", &ctx).unwrap();
    data.caches
        .catalog_page
        .insert(cache_key, rendered_html.clone())
        .await;

    HttpResponse::Ok()
        .content_type("text/html")
        .body(rendered_html)
}

// Fix: changed from `#[post]` to `#[patch]` — a partial update should use PATCH
// (or PUT for a full replacement), not POST which conventionally creates resources.
// Fix: update payload changed from `web::Query` (query string) to `web::Json`
// (request body), which is the correct extractor for a JSON update payload.
#[patch("/products/{id}")]
pub async fn update(
    data: web::Data<AppState>,
    product: web::Path<String>,
    payload: web::Json<UpdateProductDto>,
) -> impl Responder {
    match data
        .repo
        .update(product.into_inner(), payload.into_inner())
        .await
    {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(e) => {
            eprintln!("Error updating product: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

