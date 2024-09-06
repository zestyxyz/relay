mod activities;
mod actors;
mod apps;
mod db;
mod error;
mod services;

use std::env;
use std::str::FromStr;

use activitypub_federation::config::{FederationConfig, FederationMiddleware};
use activitypub_federation::http_signatures::generate_actor_keypair;
use actix_cors::Cors;
use actix_web::{web, App, HttpServer};
use dotenvy::dotenv;
use services::hello;
use sqlx::types::chrono::Utc;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

use crate::services::{
    get_activity, get_beacon, get_apps, get_relays, http_get_system_user,
    http_post_relay_inbox, new_beacon, not_found, test_follow, webfinger,
};

#[derive(Clone)]
pub struct AppState {
    db: Pool<Postgres>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv().expect("Failed to locate .env file!");

    let domain = env::var("DOMAIN").expect("DOMAIN must be set");
    let port = env::var("PORT").expect("PORT must be set");
    let protocol = env::var("PROTOCOL").expect("PROTOCOL must be set");
    let full_domain = format!("{}{}", protocol, domain);
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Error building a connection pool");

    // Insert default system user if not already exists
    match sqlx::query("SELECT * FROM relays WHERE id = 0 LIMIT 1;")
        .fetch_optional(&pool)
        .await
    {
        Ok(Some(_)) => {}
        Ok(None) => {
            let keypair = generate_actor_keypair().expect("Failed to generate actor keypair");
            sqlx::query("INSERT INTO relays VALUES (0, $1, $2, $3, $4, $5, $6, $7, $8);")
                .bind(format!("{}/relay", &full_domain))
                .bind("relay".to_string())
                .bind(format!("{}/relay/inbox", &full_domain))
                .bind(format!("{}/relay/outbox", &full_domain))
                .bind(keypair.public_key)
                .bind(Some(keypair.private_key))
                .bind(Utc::now())
                .bind(true)
                .execute(&pool)
                .await
                .expect("Error inserting default relay");
        }
        Err(e) => println!("Error locating default relay: {}", e),
    };

    let config = FederationConfig::builder()
        .domain(full_domain.clone())
        .app_data(AppState { db: pool.clone() })
        .debug(false)
        .build()
        .await?;
    println!("Server listening on: {}", full_domain);
    let _ = HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(FederationMiddleware::new(config.clone()))
            .wrap(cors)
            .service(hello)
            .service(http_get_system_user)
            .service(http_post_relay_inbox)
            .service(new_beacon)
            .service(get_beacon)
            .service(get_activity)
            .service(get_apps)
            .service(get_relays)
            .service(test_follow)
            .service(webfinger)
            .default_service(web::to(not_found))
    })
    .bind(("127.0.0.1", u16::from_str(&port).unwrap()))?
    .run()
    .await;
    Ok(())
}
