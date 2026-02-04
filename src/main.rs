mod activitypub;

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::{env, fs};

use activitypub_federation::config::{FederationConfig, FederationMiddleware};
use activitypub_federation::http_signatures::generate_actor_keypair;
use actix_cors::Cors;
use actix_web::http::header;
use actix_web::middleware::NormalizePath;
use actix_web::{web, App, HttpServer};
use dotenvy::dotenv;
use sqlx::types::chrono::Utc;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tera::Tera;

use crate::activitypub::services::{
    admin_follow, admin_page, admin_toggle_visible, get_activity, get_app, get_apps, get_beacon,
    get_image, get_relays, http_get_system_user, http_post_relay_inbox, index, login, new_beacon,
    not_found, request_login_token, update_session_info, webfinger,
};

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct SessionInfo {
    session_id: String,
    timestamp: i64,
}

#[derive(Clone)]
pub struct AppState {
    db: Pool<Postgres>,
    tera: Tera,
    debug: bool,
    show_adult_content: bool,
    is_custom_page: HashMap<String, bool>,
    sessions: Arc<RwLock<HashMap<String, Vec<SessionInfo>>>>,
    index_hide_apps_with_no_images: bool,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv().expect("Failed to locate .env file!");

    let debug = env::var("DEBUG").unwrap_or("false".to_string()) == "true";
    let domain = env::var("DOMAIN").expect("DOMAIN must be set");
    let port = env::var("PORT").expect("PORT must be set");
    let protocol = env::var("PROTOCOL").expect("PROTOCOL must be set");
    let full_domain = format!("{}{}", protocol, domain);
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let show_adult_content =
        env::var("SHOW_ADULT_CONTENT").unwrap_or("false".to_string()) == "true";
    let index_hide_apps_with_no_images =
        env::var("INDEX_HIDE_APPS_WITH_NO_IMAGES").unwrap_or("true".to_string()) == "true";
    let pool = PgPoolOptions::new()
        .max_connections(20)
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
        Err(e) => eprintln!("Error locating default relay: {}", e),
    };

    // Determine which pages are custom, if any
    let mut is_custom_page = HashMap::<String, bool>::new();
    is_custom_page.insert(
        "admin".to_string(),
        fs::exists("frontend/admin.html").unwrap(),
    );
    is_custom_page.insert("app".to_string(), fs::exists("frontend/app.html").unwrap());
    is_custom_page.insert(
        "apps".to_string(),
        fs::exists("frontend/apps.html").unwrap(),
    );
    is_custom_page.insert(
        "error".to_string(),
        fs::exists("frontend/error.html").unwrap(),
    );
    is_custom_page.insert(
        "index".to_string(),
        fs::exists("frontend/index.html").unwrap(),
    );
    is_custom_page.insert(
        "login".to_string(),
        fs::exists("frontend/login.html").unwrap(),
    );
    is_custom_page.insert(
        "relays".to_string(),
        fs::exists("frontend/relays.html").unwrap(),
    );

    // Create in-memory session store for app live counts
    let sessions = Arc::new(RwLock::new(HashMap::<String, Vec<SessionInfo>>::new()));

    let tera = Tera::new(concat!(env!("CARGO_MANIFEST_DIR"), "/frontend/**/*.html")).unwrap();

    let config = FederationConfig::builder()
        .domain(domain.clone())
        .app_data(AppState {
            db: pool.clone(),
            tera,
            debug,
            show_adult_content,
            is_custom_page,
            sessions,
            index_hide_apps_with_no_images,
        })
        .debug(debug)
        .build()
        .await?;
    // Increase max JSON payload size from 2 MB to 10 MB
    let json_config = web::JsonConfig::default().limit(1024 * 1024 * 10);
    let cors_origin = full_domain.clone();
    println!("Server listening on: {}", full_domain);
    let _ = HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin(&cors_origin)
            .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
            .allowed_headers(vec![header::AUTHORIZATION, header::ACCEPT, header::CONTENT_TYPE])
            .supports_credentials()
            .max_age(3600);
        App::new()
            .app_data(json_config.clone())
            .wrap(NormalizePath::trim())
            .wrap(FederationMiddleware::new(config.clone()))
            .wrap(cors)
            .service(index)
            .service(http_get_system_user)
            .service(http_post_relay_inbox)
            .service(new_beacon)
            .service(get_beacon)
            .service(get_activity)
            .service(get_app)
            .service(get_apps)
            .service(get_relays)
            .service(login)
            .service(request_login_token)
            .service(admin_page)
            .service(admin_follow)
            .service(admin_toggle_visible)
            .service(webfinger)
            .service(get_image)
            .service(update_session_info)
            .service(actix_files::Files::new("/static", "frontend"))
            .default_service(web::route().to(not_found))
    })
    .bind(("0.0.0.0", u16::from_str(&port).unwrap()))?
    .run()
    .await;
    Ok(())
}
