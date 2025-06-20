use actix_web::{http,web, App, HttpServer};
use actix_web::dev::Server;
use tracing_actix_web::TracingLogger;
use sqlx::PgPool;
use std::net::TcpListener;
use actix_cors::Cors;

pub mod config;
mod routes;
mod handlers;
pub mod models;
mod utils;
pub mod telemetry;
mod middleware;
mod db;
pub mod game;
pub mod league;
mod workout;
pub mod services;
use crate::routes::init_routes;
use crate::config::jwt::JwtSettings;

pub fn run(
    listener: TcpListener,
    db_pool: PgPool,
    jwt_settings: JwtSettings,
    redis_client: Option<redis::Client>
) -> Result<Server, std::io::Error> {
    // Wrap using web::Data, which boils down to an Arc smart pointer
    let db_pool = web::Data::new(db_pool);
    let jwt_settings = web::Data::new(jwt_settings);
    let redis_client = redis_client.map(|client| {
        web::Data::new(client)
    });

    let server = HttpServer::new( move || {
        let cors = Cors::default()
            .allowed_origin("http://localhost:3000")
            .allowed_origin("http://localhost:3001")
            .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "PATCH"])
            .allowed_headers(vec![
                http::header::AUTHORIZATION,
                http::header::ACCEPT,
                http::header::CONTENT_TYPE,
                http::header::UPGRADE,
                http::header::CONNECTION,
            ])
            .supports_credentials()
            .max_age(3600);

        let mut app = App::new()
            .wrap(TracingLogger::default())
            .wrap(cors)
            // Get a pointer copy and attach it to the application state
            .app_data(db_pool.clone())
            .app_data(jwt_settings.clone());
        if let Some(ref redis) = redis_client {
            app = app.app_data(redis.clone());
        }

        app.configure(init_routes)
    })
    .listen(listener)?
    .run();

    Ok(server)
}