use std::collections::{HashMap, HashSet};
use std::env;
use std::str::FromStr;
use std::time::Duration;

use rand::Rng;

use activitypub_federation::actix_web::inbox::receive_activity;
use activitypub_federation::config::Data;
use activitypub_federation::fetch::object_id::ObjectId;
use activitypub_federation::fetch::webfinger::{build_webfinger_response, extract_webfinger_name};
use activitypub_federation::kinds::activity::{CreateType, UpdateType};
use activitypub_federation::kinds::actor::ServiceType;
use activitypub_federation::protocol::context::WithContext;
use activitypub_federation::traits::{ActivityHandler, Actor};
use activitypub_federation::FEDERATION_CONTENT_TYPE;
use actix_web::cookie::{time, Cookie};
use actix_web::web::{self, Bytes};
use actix_web::{get, post, put, HttpRequest, HttpResponse, Responder};
use dataurl::DataUrl;
use jwt_simple::prelude::*;
use serde::{Deserialize, Serialize};
use tera::Context;
use url::Url;

use super::activities::{Create, Follow, Update};
use super::actors::{DbRelay, Relay};
use super::apps::{APImage, App, DbApp};
use super::db::{
    create_activity, create_app, get_activities_count, get_activity_by_id, get_all_apps,
    get_all_relays, get_app_by_base_url, get_app_by_id, get_app_by_slug, get_apps_count,
    get_relay_by_id, get_relay_followers, get_system_user, mark_app_verified, set_app_slug,
    set_verification_code, slug_exists, toggle_app_visibility, update_app, update_app_details,
};
use crate::{AppState, NewSessionEvent, SessionInfo};

#[derive(Deserialize)]
pub struct BeaconPayload {
    pub url: String,
    pub name: String,
    pub description: String,
    pub active: bool,
    pub image: Option<String>,
    pub adult: Option<bool>,
    pub tags: Option<String>,
}


#[derive(Deserialize)]
pub struct LoginPayload {
    password: String,
}

#[derive(Deserialize)]
pub struct FollowPayload {
    follow_url: String,
}

#[derive(Deserialize)]
pub struct ToggleVisibilityPayload {
    app_id: i32,
}

#[derive(Deserialize)]
pub struct SessionPayload {
    session_id: String,
    url: String,
    timestamp: i64,
}

fn template_fail_screen(e: tera::Error) -> web::Html {
    eprintln!("Template error: {:?}", e);
    web::Html::new("Failed to render to template!")
}

fn server_fail_screen(e: super::error::Error) -> web::Html {
    eprintln!("Server error: {}", e);
    web::Html::new("Server has encountered an internal error. Please check again later.")
}

/// Validates admin JWT token from request cookie
async fn validate_admin_token(request: &HttpRequest, data: &Data<AppState>) -> Result<(), HttpResponse> {
    let cookie = request.cookie("relay-admin-token");
    let token = match cookie {
        Some(c) => c.value().to_string(),
        None => return Err(HttpResponse::Unauthorized().body("No authentication token")),
    };

    let user = match get_relay_by_id(0, data).await {
        Ok(u) => u,
        Err(_) => return Err(HttpResponse::InternalServerError().body("Failed to get system user")),
    };

    let private_key = match user.private_key_pem() {
        Some(pk) => pk,
        None => return Err(HttpResponse::InternalServerError().body("System user has no private key")),
    };

    let keypair = match RS256KeyPair::from_pem(&private_key) {
        Ok(kp) => kp,
        Err(_) => return Err(HttpResponse::InternalServerError().body("Invalid system keypair")),
    };

    let public_key = keypair.public_key();
    match public_key.verify_token::<NoCustomClaims>(&token, None) {
        Ok(_) => Ok(()),
        Err(_) => Err(HttpResponse::Unauthorized().body("Invalid or expired token")),
    }
}

/// App with embedded live count for template rendering
#[derive(Serialize)]
struct AppWithCount {
    id: i32,
    url: String,
    name: String,
    description: String,
    image: String,
    live_count: usize,
    slug: Option<String>,
    page_path: String,
}

#[get("/")]
async fn index(data: Data<AppState>) -> impl Responder {
    let template_path = get_template_path(&data, "index");
    match get_all_apps(&data).await {
        Ok(mut apps) => {
            // Count total unique base URLs in the database (before filtering)
            let total_unique_apps: HashSet<String> = apps
                .iter()
                .filter_map(|app| get_base_url(&app.url))
                .collect();

            // Filter apps for display in the front carousel
            if !data.debug {
                apps.retain(|app| !app.url.contains("localhost"));
            }
            if data.index_hide_apps_with_no_images {
                apps.retain(|app| app.image != "#");
            }
            apps.retain(|app| app.visible);

            // Deduplicate apps by base URL (ignoring query parameters)
            // Keep the first app for each base URL, sum live counts
            prune_old_sessions(&data);
            let sessions = match data.sessions.read() {
                Ok(sessions) => sessions,
                Err(poisoned) => {
                    eprintln!("Warning: sessions lock was poisoned. Attempting recovery...");
                    poisoned.into_inner()
                }
            };

            let mut seen_base_urls: HashSet<String> = HashSet::new();
            let mut deduplicated_apps: Vec<(DbApp, usize)> = Vec::new();

            for app in apps.into_iter() {
                let base_url = get_base_url(&app.url).unwrap_or_else(|| app.url.clone());
                // Sum live counts from all session URLs that match this app's base URL
                let live_count: usize = sessions
                    .iter()
                    .filter(|(session_url, _)| {
                        get_base_url(session_url).as_ref() == Some(&base_url)
                    })
                    .map(|(_, session_list)| session_list.len())
                    .sum();

                if seen_base_urls.contains(&base_url) {
                    // Already have an app with this base URL, skip
                    // (live_count already includes all sessions for this base URL)
                    continue;
                }
                seen_base_urls.insert(base_url);
                deduplicated_apps.push((app, live_count));
            }

            // Sort by live count and take top 25
            deduplicated_apps.sort_by(|a, b| b.1.cmp(&a.1));
            deduplicated_apps.truncate(25);

            // Create combined app+count structs for template
            let apps_to_display: Vec<AppWithCount> = deduplicated_apps
                .iter()
                .map(|(app, count)| AppWithCount {
                    id: app.id,
                    url: app.url.clone(),
                    name: app.name.clone(),
                    description: app.description.clone(),
                    image: app.image.clone(),
                    live_count: *count,
                    slug: app.slug.clone(),
                    page_path: app.page_path(),
                })
                .collect();

            // Calculate total users online across all apps
            let total_users_online: usize = sessions
                .values()
                .map(|app_sessions| app_sessions.len())
                .sum();

            // Render
            let mut ctx = tera::Context::new();
            ctx.insert("apps_count", &total_unique_apps.len());
            ctx.insert("total_users_online", &total_users_online);

            ctx.insert("apps", &apps_to_display);
            ctx.insert("google_analytics_id", &data.google_analytics_id);

            match data.tera.render(&template_path, &ctx) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
        Err(e) => server_fail_screen(e),
    }
}

#[derive(Serialize)]
struct ApiApp {
    name: String,
    url: String,
    image: String,
    live_count: usize,
}

#[derive(Serialize)]
struct ApiAppsResponse {
    apps: Vec<ApiApp>,
    total_apps: usize,
    total_users_online: usize,
}

#[get("/api/apps")]
pub async fn api_get_apps(data: Data<AppState>) -> impl Responder {
    match get_all_apps(&data).await {
        Ok(mut apps) => {
            // Filter apps
            if !data.debug {
                apps.retain(|app| !app.url.contains("localhost"));
            }
            if data.index_hide_apps_with_no_images {
                apps.retain(|app| app.image != "#");
            }
            apps.retain(|app| app.visible);

            // Deduplicate by hostname
            let mut unique_urls = HashSet::new();
            apps.retain(|app| {
                let url = normalize_app_url(app.url.clone());
                match Url::parse(&url) {
                    Ok(parsed_url) => {
                        if let Some(host) = parsed_url.host_str() {
                            unique_urls.insert(host.to_string())
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                }
            });

            // Get live counts
            prune_old_sessions(&data);
            let sessions = match data.sessions.read() {
                Ok(sessions) => sessions,
                Err(poisoned) => poisoned.into_inner(),
            };

            let mut app_to_live_count: Vec<(DbApp, usize)> = apps
                .into_iter()
                .map(|app| {
                    let base_url = get_base_url(&app.url).unwrap_or_else(|| app.url.clone());
                    // Sum live counts from all session URLs that match this app's base URL
                    let live_count: usize = sessions
                        .iter()
                        .filter(|(session_url, _)| {
                            get_base_url(session_url).as_ref() == Some(&base_url)
                        })
                        .map(|(_, session_list)| session_list.len())
                        .sum();
                    (app, live_count)
                })
                .collect();

            // Sort by live count descending
            app_to_live_count.sort_by(|a, b| b.1.cmp(&a.1));

            // Take top 10
            app_to_live_count.truncate(10);

            let total_users_online: usize = sessions.values().map(|s| s.len()).sum();
            let total_apps = unique_urls.len();

            let api_apps: Vec<ApiApp> = app_to_live_count
                .into_iter()
                .map(|(app, live_count)| ApiApp {
                    name: app.name,
                    url: normalize_app_url(app.url),
                    image: app.image,
                    live_count,
                })
                .collect();

            HttpResponse::Ok().json(ApiAppsResponse {
                apps: api_apps,
                total_apps,
                total_users_online,
            })
        }
        Err(e) => {
            eprintln!("API error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch apps"
            }))
        }
    }
}

#[get("/relay/beacon/{id}")]
async fn get_beacon(info: web::Path<i32>, data: Data<AppState>) -> impl Responder {
    match get_app_by_id(info.into_inner() + 1, &data).await {
        Ok(app) => {
            let app_image = (!app.image.is_empty()).then(|| APImage::new(app.image));
            HttpResponse::Ok()
                .content_type(FEDERATION_CONTENT_TYPE)
                .json(App::new(
                    app.id,
                    app.ap_id,
                    String::new(),
                    vec![],
                    app.url,
                    app.name,
                    app.description,
                    app_image,
                    app.adult,
                    app.tags,
                ))
        }
        Err(e) => {
            eprintln!("Error fetching app from DB: {}", e);
            HttpResponse::NotFound().body("No beacon found")
        }
    }
}

#[put("/beacon")]
async fn new_beacon(
    req: HttpRequest,
    data: Data<AppState>,
    req_body: web::Json<BeaconPayload>,
) -> impl Responder {
    // Env vars
    let relay_domain = env::var("DOMAIN").expect("DOMAIN must be set");
    let protocol = env::var("PROTOCOL").expect("PROTOCOL must be set");

    // Extract fields from request body
    let url = req_body.url.clone();

    // Validate that the Origin header matches the URL being registered
    // This ensures browsers can only register the domain they're actually running on
    if let Some(origin_header) = req.headers().get("Origin") {
        if let Ok(origin_str) = origin_header.to_str() {
            if let (Ok(origin_url), Ok(payload_url)) = (Url::parse(origin_str), Url::parse(&url)) {
                // Compare hosts, stripping www. prefix for flexibility
                let origin_host = origin_url.host_str().unwrap_or("").trim_start_matches("www.");
                let payload_host = payload_url.host_str().unwrap_or("").trim_start_matches("www.");
                if origin_host != payload_host {
                    eprintln!("Beacon rejected: Origin '{}' does not match URL '{}'", origin_str, url);
                    return HttpResponse::Forbidden()
                        .body("Origin header does not match the URL being registered");
                }
            }
        }
    }
    println!("Beacon request received for: {}", url);
    let name = req_body.name.clone();
    let description = req_body.description.clone();
    let active = req_body.active;
    let image = req_body.image.clone().unwrap_or("#".to_string());
    let adult = req_body.adult.unwrap_or(false);
    let tags = req_body.tags.clone().unwrap_or("".to_string());

    // Query system user and DB information
    let system_user = match get_system_user(&data).await {
        Ok(user) => user,
        Err(e) => {
            eprintln!("Error fetching system user: {}", e);
            return HttpResponse::InternalServerError().body("Failed to get system user");
        }
    };
    let domain = system_user.ap_id.inner().as_str();
    let apps_count = match get_apps_count(&data).await {
        Ok(count) => count,
        Err(e) => {
            eprintln!("Error fetching apps count: {}", e);
            return HttpResponse::InternalServerError().body("Failed to get apps count");
        }
    };
    let activities_count: i64 = match get_activities_count(&data).await {
        Ok(count) => count,
        Err(e) => {
            eprintln!("Error fetching activities count: {}", e);
            return HttpResponse::InternalServerError().body("Failed to get activities count");
        }
    };

    // Check if app with same base URL already exists (ignoring query parameters)
    // If it does and nothing changed, return 304
    // Otherwise, update the DB and send the relevant activities
    let base_url = get_base_url(&url).unwrap_or_else(|| url.clone());
    match get_app_by_base_url(&data, &base_url).await {
        Ok(Some(app)) => {
            // Set up references to the latest values for each field
            let app_name = &get_latest_value(app.name.clone(), name.clone());
            let app_description = &get_latest_value(app.description.clone(), description.clone());
            let app_active = get_latest_value(app.active, active);
            let app_image = if app.image == image || image == "#" {
                &app.image
            } else {
                &image
            };
            let app_adult = get_latest_value(app.adult, adult);
            let app_tags = get_latest_value(app.tags.clone(), tags.clone());

            // Parse optionally attached image to see if we need to save a copy locally
            let image = if app.image != image && app_image.contains("data:") {
                let image_url = create_local_image(
                    app.ap_id.clone().into_inner().as_str(),
                    &protocol,
                    &relay_domain,
                    app_image,
                );
                if image_url.is_empty() {
                    eprintln!("Error creating local image");
                    return HttpResponse::BadRequest().finish();
                }

                image_url
            } else {
                app_image.clone()
            };

            // Check if no fields have changed, in which case exit early
            if app_name == &app.name
                && app_description == &app.description
                && app_active == app.active
                && image == app.image
                && app_adult == app.adult
                && app_tags == app.tags
            {
                return HttpResponse::NotModified().finish();
            }

            match update_app(
                &data,
                url.clone(),
                app_name.clone(),
                app_description.clone(),
                app_active,
                image,
                app_adult,
                app_tags.clone(),
            )
            .await
            {
                Ok(_) => {
                    let activity = Update {
                        actor: system_user.ap_id.clone(),
                        object: app.ap_id.clone(),
                        kind: UpdateType::Update,
                        id: Url::from_str(&format!(
                            "{}/activities/{}",
                            domain,
                            activities_count + 1
                        ))
                        .unwrap(),
                    };
                    match create_activity(
                        &data,
                        format!(
                            "{}/activities/{}",
                            system_user.ap_id.inner().as_str(),
                            activities_count + 1
                        ),
                        system_user.ap_id.inner().as_str(),
                        app.ap_id.inner().as_str(),
                        "Update",
                    )
                    .await
                    {
                        Ok(_) => {
                            let recipients: Vec<DbRelay> = match get_relay_followers(&data).await {
                                Ok(relays) => relays,
                                Err(e) => {
                                    eprintln!("Error fetching relays: {}", e);
                                    vec![]
                                }
                            };
                            let recipient_inboxes: Vec<Url> =
                                recipients.iter().map(|relay| relay.inbox.clone()).collect();
                            let _ = system_user
                                .send(activity, recipient_inboxes, false, &data)
                                .await
                                .map_err(|e| eprintln!("Error sending activity: {}", e));
                        }
                        Err(e) => {
                            eprintln!("Error creating activity: {}", e);
                            return HttpResponse::InternalServerError().body(e.to_string());
                        }
                    }

                    return HttpResponse::Ok().finish();
                }
                Err(e) => eprintln!("Error updating app: {}", e),
            }
        }
        Ok(None) => {
            // App doesn't exist, will be created below
        }
        Err(e) => eprintln!("Error fetching app from DB: {}", e),
    }

    // At this point, it should be certain that the app doesn't already exist.
    // Create a new app and send the Create activity to following relays
    let ap_id = format!("{}/beacon/{}", domain, apps_count);
    let image_url = if image.contains("data:") {
        let image_url = create_local_image(&ap_id, &protocol, &relay_domain, &image);
        if image_url.is_empty() {
            eprintln!("Error creating local image");
            return HttpResponse::BadRequest().finish();
        }
        image_url
    } else {
        image
    };

    match create_app(
        &data,
        ap_id,
        url,
        name.clone(),
        description,
        active,
        image_url,
        adult,
        tags.clone(),
    )
    .await
    {
        Ok(_) => {
            // Generate and set a unique slug for the new app
            let slug = generate_unique_slug(&data, &name).await;
            // App ID is apps_count + 1 since we just created a new one
            if let Err(e) = set_app_slug(&data, (apps_count + 1) as i32, &slug).await {
                eprintln!("Error setting slug for new app: {}", e);
            }
        }
        Err(e) => eprintln!("Error inserting new beacon: {}", e),
    };
    let activity = Create {
        actor: ObjectId::parse(domain).unwrap(),
        object: ObjectId::parse(&format!("{}/beacon/{}", domain, apps_count)).unwrap(),
        kind: CreateType::Create,
        id: Url::from_str(&format!("{}/activities/{}", domain, activities_count)).unwrap(),
    };
    let recipients: Vec<DbRelay> = match get_relay_followers(&data).await {
        Ok(relays) => relays,
        Err(e) => {
            eprintln!("Error fetching relays: {}", e);
            vec![]
        }
    };
    let recipient_inboxes: Vec<Url> = recipients.iter().map(|relay| relay.inbox.clone()).collect();
    let _ = system_user
        .send(activity, recipient_inboxes, false, &data)
        .await
        .map_err(|e| eprintln!("Error sending activity: {}", e));

    HttpResponse::Ok().finish()
}

#[get("/world/{id_or_slug}")]
pub async fn get_world(data: Data<AppState>, path: web::Path<String>) -> impl Responder {
    get_app_handler(data, path).await
}

#[get("/app/{id_or_slug}")]
async fn get_app(data: Data<AppState>, path: web::Path<String>) -> impl Responder {
    get_app_handler(data, path).await
}

async fn get_app_handler(data: Data<AppState>, path: web::Path<String>) -> impl Responder {
    let template_path = get_template_path(&data, "app");
    let error_path = get_template_path(&data, "error");

    let id_or_slug = path.into_inner();

    // Try parsing as ID first, otherwise treat as slug
    let app_result = if let Ok(id) = id_or_slug.parse::<i32>() {
        get_app_by_id(id + 1, &data).await.ok()
    } else {
        match get_app_by_slug(&data, &id_or_slug).await {
            Ok(Some(app)) => Some(app),
            _ => None,
        }
    };

    match app_result {
        Some(app) => {
            prune_old_sessions(&data);
            let sessions = match data.sessions.read() {
                Ok(sessions) => sessions,
                Err(poisoned) => {
                    eprintln!("Warning: sessions lock was poisoned. Attempting recovery...");
                    poisoned.into_inner()
                }
            };
            let base_url = get_base_url(&app.url).unwrap_or_else(|| app.url.clone());
            // Sum live counts from all session URLs that match this app's base URL
            let live_count: usize = sessions
                .iter()
                .filter(|(session_url, _)| {
                    get_base_url(session_url).as_ref() == Some(&base_url)
                })
                .map(|(_, session_list)| session_list.len())
                .sum();
            let url = normalize_app_url(app.url.clone());
            let mut ctx = tera::Context::new();
            ctx.insert("name", &app.name);
            ctx.insert("description", &app.description);
            ctx.insert("url", &url);
            ctx.insert("image", &app.image);
            ctx.insert("live_count", &live_count);
            ctx.insert("created_at", &app.created_at);
            ctx.insert("slug", &app.slug);
            ctx.insert("app_id", &app.id);
            match data.tera.render(&template_path, &ctx) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
        None => {
            eprintln!("App not found: {}", id_or_slug);
            match data.tera.render(&error_path, &Context::new()) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
    }
}

#[get("/worlds")]
pub async fn get_worlds(data: Data<AppState>) -> impl Responder {
    get_apps_handler(data).await
}

#[get("/apps")]
async fn get_apps(data: Data<AppState>) -> impl Responder {
    get_apps_handler(data).await
}

async fn get_apps_handler(data: Data<AppState>) -> impl Responder {
    let template_path = get_template_path(&data, "apps");
    let error_path = get_template_path(&data, "error");
    match get_all_apps(&data).await {
        Ok(apps) => {
            // First deduplicate by base URL (ignoring query parameters)
            let mut seen_base_urls: HashSet<String> = HashSet::new();
            let mut deduplicated_apps: Vec<DbApp> = Vec::new();
            let mut app_page_urls: HashMap<String, String> = HashMap::new();

            for app in apps.into_iter() {
                let base_url = get_base_url(&app.url).unwrap_or_else(|| app.url.clone());
                app_page_urls.insert(app.url.clone(), app.page_path());

                if seen_base_urls.insert(base_url) {
                    // First time seeing this base URL, keep this app
                    deduplicated_apps.push(app);
                }
            }

            // Group deduplicated apps by domain
            let mut domain_groups: HashMap<String, Vec<DbApp>> = HashMap::new();
            for app in deduplicated_apps.into_iter() {
                let domain = get_domain(&app.url).unwrap_or_else(|| app.url.clone());
                domain_groups
                    .entry(domain)
                    .or_insert_with(Vec::new)
                    .push(app);
            }

            // Sort groups by domain, and apps within groups by name
            let mut sorted_groups: Vec<(String, Vec<DbApp>)> = domain_groups.into_iter().collect();
            sorted_groups.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
            for (_, apps) in sorted_groups.iter_mut() {
                apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }

            let domains: Vec<String> = sorted_groups.iter().map(|(d, _)| d.clone()).collect();
            let app_groups: Vec<Vec<DbApp>> = sorted_groups.into_iter().map(|(_, v)| v).collect();

            let mut ctx = tera::Context::new();
            ctx.insert("apps", &app_groups);
            ctx.insert("domains", &domains);
            ctx.insert("app_pages", &app_page_urls);
            ctx.insert("DEBUG", &data.debug);
            ctx.insert("SHOW_ADULT_CONTENT", &data.show_adult_content);
            match data.tera.render(&template_path, &ctx) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
        Err(e) => {
            eprintln!("Error fetching apps from DB: {}", e);
            match data.tera.render(&error_path, &Context::new()) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
    }
}

#[get("/relays")]
async fn get_relays(data: Data<AppState>) -> impl Responder {
    let template_path = get_template_path(&data, "relays");
    let error_path = get_template_path(&data, "error");
    match get_all_relays(&data).await {
        Ok(relays) => {
            let mut ctx = tera::Context::new();
            ctx.insert("relays", &relays);
            match data.tera.render(&template_path, &ctx) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
        Err(e) => {
            eprintln!("Error fetching relays from DB: {}", e);
            match data.tera.render(&error_path, &Context::new()) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
    }
}

/// Handles requests to fetch system user json over HTTP
#[get("/relay")]
async fn http_get_system_user(data: Data<AppState>) -> impl Responder {
    let user = get_relay_by_id(0, &data)
        .await
        .expect("Failed to get system user!");
    let json_user = Relay {
        id: user.ap_id.clone(),
        kind: ServiceType::Service,
        preferred_username: user.name.clone(),
        name: user.name.clone(),
        inbox: user.inbox.clone(),
        outbox: user.outbox.clone(),
        public_key: user.public_key(),
    };
    HttpResponse::Ok()
        .content_type(FEDERATION_CONTENT_TYPE)
        .json(WithContext::new_default(json_user))
}

#[get("relay/activities/{id}")]
async fn get_activity(info: web::Path<i32>, data: Data<AppState>) -> impl Responder {
    match get_activity_by_id(info.into_inner(), &data).await {
        Ok(activity) => HttpResponse::Ok()
            .content_type(FEDERATION_CONTENT_TYPE)
            .json(activity),
        Err(e) => {
            eprintln!("Error fetching activity: {}", e);
            HttpResponse::NotFound().body("No activity found")
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
#[enum_delegate::implement(ActivityHandler)]
pub enum RelayAcceptedActivities {
    Follow(Follow),
    Create(Create),
    Update(Update),
}

#[post("/relay/inbox")]
async fn http_post_relay_inbox(
    request: HttpRequest,
    body: Bytes,
    data: Data<AppState>,
) -> HttpResponse {
    match receive_activity::<WithContext<RelayAcceptedActivities>, DbRelay, AppState>(
        request, body, &data,
    )
    .await
    {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub async fn not_found(request: HttpRequest, data: Data<AppState>) -> impl Responder {
    println!(
        "Got request for unknown route: {} {}",
        request.uri().path(),
        request.method().as_str()
    );
    let error_path = get_template_path(&data, "error");
    match data.tera.render(&error_path, &Context::new()) {
        Ok(html) => web::Html::new(html),
        Err(e) => template_fail_screen(e),
    }
}

#[get("/login")]
async fn login(_request: HttpRequest, data: Data<AppState>) -> impl Responder {
    let template_path = get_template_path(&data, "login");
    match data.tera.render(&template_path, &Context::new()) {
        Ok(html) => web::Html::new(html),
        Err(e) => template_fail_screen(e),
    }
}

#[post("/login")]
async fn request_login_token(
    data: Data<AppState>,
    req_body: web::Form<LoginPayload>,
) -> impl Responder {
    let user = get_relay_by_id(0, &data)
        .await
        .expect("Failed to get system user!");
    let password = std::env::var("ADMIN_PASSWORD").expect("ADMIN_PASSWORD must be set");
    if password != req_body.password {
        return HttpResponse::Unauthorized().body("Invalid password");
    }

    let duration = jwt_simple::prelude::Duration::from_days(1);
    let claim = Claims::create(duration);
    let keypair = RS256KeyPair::from_pem(&user.private_key_pem().unwrap()).unwrap();
    let token = keypair.sign(claim).unwrap();

    HttpResponse::Found() // HTTP 302 redirect to /admin
        .append_header(("Location", "/admin"))
        .cookie(
            Cookie::build("relay-admin-token", token)
                .path("/")
                .http_only(true)
                .max_age(time::Duration::days(1))
                .finish(),
        )
        .finish()
}

#[get("/images/{id}")]
async fn get_image(request: HttpRequest, _data: Data<AppState>) -> impl Responder {
    let id = request.match_info().get("id").unwrap_or("");

    // Sanitize the ID to prevent path traversal attacks
    // Only allow alphanumeric characters, dots, hyphens, and underscores
    if id.is_empty() || id.contains("..") || id.contains('/') || id.contains('\\') {
        return HttpResponse::BadRequest().body("Invalid image ID");
    }

    // Additional validation: ensure ID only contains safe characters
    if !id.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_') {
        return HttpResponse::BadRequest().body("Invalid image ID");
    }

    let image_url = format!("images/{}", id);
    let image = match std::fs::read(&image_url) {
        Ok(image_bytes) => image_bytes,
        Err(_) => {
            eprintln!("Failed to load image at: {}", image_url);
            std::fs::read("frontend/images/noimage.png").expect("Failed to load placeholder image")
        }
    };
    let mime = match image_url.rsplit_once('.').map(|(_, ext)| ext) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        _ => "image/jpeg",
    };
    HttpResponse::Ok().content_type(mime).body(image)
}

#[get("/admin")]
async fn admin_page(request: HttpRequest, data: Data<AppState>) -> impl Responder {
    let template_path = get_template_path(&data, "admin");

    // Validate JWT token
    if let Err(response) = validate_admin_token(&request, &data).await {
        // If no token at all, redirect to login
        if request.cookie("relay-admin-token").is_none() {
            return HttpResponse::TemporaryRedirect()
                .append_header(("Location", "/login"))
                .finish();
        }
        return response;
    }

    match get_all_apps(&data).await {
        Ok(apps) => {
            let mut ctx = tera::Context::new();
            ctx.insert("apps", &apps);
            match data.tera.render(&template_path, &ctx) {
                Ok(html) => HttpResponse::Ok().body(html),
                Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            }
        }
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[derive(Deserialize)]
pub struct WebfingerQuery {
    resource: String,
}

#[get("/.well-known/webfinger")]
async fn webfinger(query: web::Query<WebfingerQuery>, data: Data<AppState>) -> impl Responder {
    let name = match extract_webfinger_name(&query.resource, &data) {
        Ok(name) => name,
        Err(e) => {
            eprintln!("Error during webfinger lookup: {:?}", e);
            ""
        }
    };
    if name != "relay" {
        return HttpResponse::NotFound().finish();
    }
    let db_user = match get_system_user(&data).await {
        Ok(user) => user,
        Err(e) => {
            eprintln!("Error fetching system user: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };
    HttpResponse::Ok().json(build_webfinger_response(
        query.resource.clone(),
        db_user.ap_id.into_inner(),
    ))
}

#[post("/session")]
async fn update_session_info(
    _request: HttpRequest,
    req_body: web::Json<SessionPayload>,
    data: Data<AppState>,
) -> HttpResponse {
    let session_info = SessionInfo {
        session_id: req_body.session_id.clone(),
        timestamp: req_body.timestamp,
    };

    let is_new_session = {
        let mut sessions = match data.sessions.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Warning: sessions lock was poisoned. Attempting recovery...");
                poisoned.into_inner()
            }
        };

        match sessions.get_mut(&req_body.url) {
            Some(vec) => {
                match vec
                    .iter_mut()
                    .find(|info| info.session_id == req_body.session_id)
                {
                    Some(session) => {
                        session.timestamp = req_body.timestamp;
                        false
                    }
                    None => {
                        vec.push(session_info);
                        true
                    }
                }
            }
            None => {
                sessions.insert(req_body.url.clone(), vec![session_info]);
                true
            }
        }
    };

    // Broadcast to SSE subscribers when a new user joins
    if is_new_session {
        let app_name = match get_app_by_base_url(&data, &req_body.url).await {
            Ok(Some(app)) => app.name,
            _ => get_domain(&req_body.url).unwrap_or_else(|| "an app".to_string()),
        };

        let _ = data.new_session_tx.send(NewSessionEvent {
            app_name,
            app_url: req_body.url.clone(),
        });
    }

    HttpResponse::Ok().finish()
}

/// SSE endpoint for browsers to receive real-time session notifications
#[get("/events/sessions")]
pub async fn session_events(data: Data<AppState>) -> HttpResponse {
    let mut rx = data.new_session_tx.subscribe();

    let stream = async_stream::stream! {
        let mut interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if let Ok(json) = serde_json::to_string(&event) {
                                yield Ok::<_, std::convert::Infallible>(
                                    web::Bytes::from(format!("data: {}\n\n", json))
                                );
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = interval.tick() => {
                    yield Ok::<_, std::convert::Infallible>(web::Bytes::from(": heartbeat\n\n"));
                }
            }
        }
    };

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(stream)
}

#[post("/admin/follow")]
async fn admin_follow(
    request: HttpRequest,
    req_body: web::Form<FollowPayload>,
    data: Data<AppState>,
) -> HttpResponse {
    // Validate JWT token
    if let Err(response) = validate_admin_token(&request, &data).await {
        return response;
    }

    let db_user = match get_system_user(&data).await {
        Ok(user) => user,
        Err(e) => return HttpResponse::InternalServerError().body(format!("Failed to get system user: {}", e)),
    };

    let mut ctx = tera::Context::new();
    ctx.insert("message", "Successfully followed!");
    let template_path = get_template_path(&data, "admin");
    match db_user.follow(&req_body.follow_url, &data).await {
        Ok(_) => match data.tera.render(&template_path, &ctx) {
            Ok(html) => HttpResponse::Ok().body(html),
            Err(e) => {
                eprintln!("Template error: {}", e);
                HttpResponse::InternalServerError().body(e.to_string())
            }
        },
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[post("/admin/togglevisible")]
async fn admin_toggle_visible(
    request: HttpRequest,
    req_body: web::Form<ToggleVisibilityPayload>,
    data: Data<AppState>,
) -> HttpResponse {
    // Validate JWT token
    if let Err(response) = validate_admin_token(&request, &data).await {
        return response;
    }

    match toggle_app_visibility(req_body.app_id, &data).await {
        Ok(_) => {
            let template_path = get_template_path(&data, "admin");
            match get_all_apps(&data).await {
                Ok(apps) => {
                    let mut ctx = tera::Context::new();
                    ctx.insert("apps", &apps);
                    match data.tera.render(&template_path, &ctx) {
                        Ok(html) => HttpResponse::Ok().body(html),
                        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
                    }
                }
                Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            }
        }
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

// ============================================================================
// Owner Verification and Editing Endpoints
// ============================================================================

#[derive(Serialize, Deserialize)]
struct OwnerClaims {
    app_id: i32,
    slug: String,
}

/// Validate owner token from cookie
async fn validate_owner_token(
    request: &HttpRequest,
    data: &Data<AppState>,
    expected_app_id: i32,
) -> Result<OwnerClaims, HttpResponse> {
    let cookie = request.cookie("relay-owner-token");
    let token = match cookie {
        Some(c) => c.value().to_string(),
        None => return Err(HttpResponse::Unauthorized().body("No owner token")),
    };

    let user = match get_relay_by_id(0, data).await {
        Ok(u) => u,
        Err(_) => {
            return Err(HttpResponse::InternalServerError().body("Failed to get system user"))
        }
    };

    let private_key = match user.private_key_pem() {
        Some(pk) => pk,
        None => {
            return Err(HttpResponse::InternalServerError().body("System user has no private key"))
        }
    };

    let keypair = match RS256KeyPair::from_pem(&private_key) {
        Ok(kp) => kp,
        Err(_) => return Err(HttpResponse::InternalServerError().body("Invalid system keypair")),
    };

    let public_key = keypair.public_key();
    let claims = match public_key.verify_token::<OwnerClaims>(&token, None) {
        Ok(c) => c,
        Err(_) => return Err(HttpResponse::Unauthorized().body("Invalid or expired token")),
    };

    // Verify the token is for the expected app
    if claims.custom.app_id != expected_app_id {
        return Err(HttpResponse::Unauthorized().body("Token not valid for this world"));
    }

    Ok(claims.custom)
}

/// Create owner token (JWT)
async fn create_owner_token(
    data: &Data<AppState>,
    app_id: i32,
    slug: &str,
) -> Result<String, HttpResponse> {
    let user = match get_relay_by_id(0, data).await {
        Ok(u) => u,
        Err(_) => {
            return Err(HttpResponse::InternalServerError().body("Failed to get system user"))
        }
    };

    let private_key = match user.private_key_pem() {
        Some(pk) => pk,
        None => {
            return Err(HttpResponse::InternalServerError().body("System user has no private key"))
        }
    };

    let keypair = match RS256KeyPair::from_pem(&private_key) {
        Ok(kp) => kp,
        Err(_) => return Err(HttpResponse::InternalServerError().body("Invalid system keypair")),
    };

    let claims = OwnerClaims {
        app_id,
        slug: slug.to_string(),
    };

    // Token valid for 7 days
    let duration = jwt_simple::prelude::Duration::from_days(7);
    let jwt_claims = Claims::with_custom_claims(claims, duration);

    match keypair.sign(jwt_claims) {
        Ok(token) => Ok(token),
        Err(_) => Err(HttpResponse::InternalServerError().body("Failed to sign token")),
    }
}

#[derive(Deserialize)]
pub struct EditWorldQuery {
    verified: Option<bool>,
}

/// Show edit page - either verification instructions or edit form
#[get("/world/{slug}/edit")]
pub async fn get_world_edit(
    request: HttpRequest,
    data: Data<AppState>,
    path: web::Path<String>,
    query: web::Query<EditWorldQuery>,
) -> impl Responder {
    let template_path = get_template_path(&data, "edit");
    let error_path = get_template_path(&data, "error");
    let slug = path.into_inner();

    // Get app by slug or ID
    let app = if let Ok(id) = slug.parse::<i32>() {
        get_app_by_id(id + 1, &data).await.ok()
    } else {
        match get_app_by_slug(&data, &slug).await {
            Ok(Some(app)) => Some(app),
            _ => None,
        }
    };

    let app = match app {
        Some(a) => a,
        None => {
            return match data.tera.render(&error_path, &Context::new()) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
    };

    // Check if user has valid owner token
    let is_verified = validate_owner_token(&request, &data, app.id).await.is_ok();

    let mut ctx = tera::Context::new();
    ctx.insert("name", &app.name);
    ctx.insert("description", &app.description);
    ctx.insert("url", &app.url);
    ctx.insert("image", &app.image);
    ctx.insert("tags", &app.tags);
    ctx.insert("adult", &app.adult);
    ctx.insert("slug", &app.slug);
    ctx.insert("app_id", &app.id);
    ctx.insert("is_verified", &is_verified);
    ctx.insert("verification_code", &app.verification_code);
    ctx.insert("just_verified", &query.verified.unwrap_or(false));

    match data.tera.render(&template_path, &ctx) {
        Ok(html) => web::Html::new(html),
        Err(e) => template_fail_screen(e),
    }
}

/// Request verification code for a world
#[post("/world/{slug}/request-verification")]
pub async fn request_world_verification(
    data: Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let slug = path.into_inner();

    // Get app by slug or ID
    let app = if let Ok(id) = slug.parse::<i32>() {
        get_app_by_id(id + 1, &data).await.ok()
    } else {
        match get_app_by_slug(&data, &slug).await {
            Ok(Some(app)) => Some(app),
            _ => None,
        }
    };

    let app = match app {
        Some(a) => a,
        None => return HttpResponse::NotFound().body("World not found"),
    };

    // Generate verification code if not already set
    let code = match &app.verification_code {
        Some(c) if !c.is_empty() => c.clone(),
        _ => {
            let new_code = generate_verification_code();
            if let Err(e) = set_verification_code(&data, app.id, &new_code).await {
                eprintln!("Error setting verification code: {}", e);
                return HttpResponse::InternalServerError().body("Failed to set verification code");
            }
            new_code
        }
    };

    HttpResponse::Ok().json(serde_json::json!({
        "code": code,
        "url": app.url,
        "instruction": format!(
            "Add this meta tag to the <head> of your site at {}:\n<meta name=\"zesty-verify\" content=\"{}\">",
            app.url, code
        )
    }))
}

/// Verify world ownership by checking meta tag
#[post("/world/{slug}/verify")]
pub async fn verify_world_ownership(data: Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let slug = path.into_inner();

    // Get app by slug or ID
    let app = if let Ok(id) = slug.parse::<i32>() {
        get_app_by_id(id + 1, &data).await.ok()
    } else {
        match get_app_by_slug(&data, &slug).await {
            Ok(Some(app)) => Some(app),
            _ => None,
        }
    };

    let app = match app {
        Some(a) => a,
        None => return HttpResponse::NotFound().body("World not found"),
    };

    let verification_code = match &app.verification_code {
        Some(c) if !c.is_empty() => c.clone(),
        _ => return HttpResponse::BadRequest().body("No verification code set. Request one first."),
    };

    // Fetch the world's URL and check for the meta tag
    let url = normalize_app_url(app.url.clone());
    let response = match reqwest::get(&url).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error fetching URL {}: {}", url, e);
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Could not fetch your site. Make sure it's accessible.",
                "details": e.to_string()
            }));
        }
    };

    let html = match response.text().await {
        Ok(h) => h,
        Err(e) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Could not read response from your site.",
                "details": e.to_string()
            }))
        }
    };

    // Parse HTML and look for zesty-verify meta tag
    let document = scraper::Html::parse_document(&html);
    let selector = match scraper::Selector::parse("meta[name=\"zesty-verify\"]") {
        Ok(s) => s,
        Err(_) => return HttpResponse::InternalServerError().body("Selector parse error"),
    };

    let found_code = document
        .select(&selector)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    match found_code {
        Some(code) if code == verification_code => {
            // Verification successful! Mark as verified and create token
            if let Err(e) = mark_app_verified(&data, app.id).await {
                eprintln!("Error marking app verified: {}", e);
                return HttpResponse::InternalServerError().body("Failed to save verification");
            }

            let app_slug = app.slug.clone().unwrap_or_else(|| app.id.to_string());
            let token = match create_owner_token(&data, app.id, &app_slug).await {
                Ok(t) => t,
                Err(r) => return r,
            };

            HttpResponse::Ok()
                .cookie(
                    Cookie::build("relay-owner-token", token)
                        .path("/")
                        .http_only(true)
                        .max_age(time::Duration::days(7))
                        .finish(),
                )
                .json(serde_json::json!({
                    "success": true,
                    "message": "Verification successful! You can now edit this world."
                }))
        }
        Some(code) => HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Verification code mismatch",
            "found": code,
            "expected": verification_code
        })),
        None => HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Meta tag not found",
            "instruction": format!(
                "Add this to the <head> of {}:\n<meta name=\"zesty-verify\" content=\"{}\">",
                url, verification_code
            )
        })),
    }
}

#[derive(Deserialize)]
pub struct UpdateWorldPayload {
    name: String,
    description: String,
    image: Option<String>,
    tags: Option<String>,
    adult: Option<bool>,
}

/// Update world details (requires owner token)
#[post("/world/{slug}/update")]
pub async fn update_world(
    request: HttpRequest,
    data: Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateWorldPayload>,
) -> HttpResponse {
    let slug = path.into_inner();

    // Get app by slug or ID
    let app = if let Ok(id) = slug.parse::<i32>() {
        get_app_by_id(id + 1, &data).await.ok()
    } else {
        match get_app_by_slug(&data, &slug).await {
            Ok(Some(app)) => Some(app),
            _ => None,
        }
    };

    let app = match app {
        Some(a) => a,
        None => return HttpResponse::NotFound().body("World not found"),
    };

    // Validate owner token
    if let Err(response) = validate_owner_token(&request, &data, app.id).await {
        return response;
    }

    // Update the app details
    let image = payload.image.clone().unwrap_or_else(|| app.image.clone());
    let tags = payload.tags.clone().unwrap_or_else(|| app.tags.clone());
    let adult = payload.adult.unwrap_or(app.adult);

    if let Err(e) =
        update_app_details(&data, app.id, &payload.name, &payload.description, &image, &tags, adult)
            .await
    {
        eprintln!("Error updating app: {}", e);
        return HttpResponse::InternalServerError().body("Failed to update world");
    }

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "World updated successfully"
    }))
}

fn get_template_path(data: &Data<AppState>, page: &str) -> String {
    if *data.is_custom_page.get(page).unwrap() {
        format!("{}.html", page)
    } else {
        format!("{}.default.html", page)
    }
}

fn create_local_image(ap_id: &str, protocol: &str, relay_domain: &str, app_image: &str) -> String {
    // Get app ID by splitting off from Activitypub ID
    let count = ap_id.split("/").last().unwrap();
    // Construct filepath to images folder
    let filepath = format!("images/{}.png", count);
    // Construct external URL
    let image_url = format!("{}{}/{}", protocol, relay_domain, filepath);
    if std::fs::exists(&filepath).unwrap() {
        // Image already exists, return image URL
        return image_url;
    }
    let dataurl = match DataUrl::parse(app_image) {
        Ok(dataurl) => dataurl,
        Err(e) => {
            eprintln!("Error parsing image data: {:?}", e);
            return String::new();
        }
    };
    let _ = std::fs::write(&filepath, dataurl.get_data());
    image_url
}

fn get_latest_value<T: PartialEq>(original: T, incoming: T) -> T {
    if original != incoming {
        incoming
    } else {
        original
    }
}

fn prune_old_sessions(data: &Data<AppState>) {
    let mut sessions = match data.sessions.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("Warning: sessions lock was poisoned during pruning. Attempting recovery...");
            poisoned.into_inner()
        }
    };
    // Iterate through all sessions and remove any that are older than 5 seconds
    sessions.values_mut().for_each(|url_sessions| {
        url_sessions.retain(|session| {
            (time::OffsetDateTime::now_utc().unix_timestamp() * 1000) - session.timestamp < 5000
        })
    });
}

fn normalize_app_url(url: String) -> String {
    if !url.starts_with("https") && !url.starts_with("http") {
        let mut adjusted_url = String::new();
        adjusted_url.push_str("https://");
        adjusted_url.push_str(&url);
        adjusted_url
    } else {
        url
    }
}

/// Extracts base URL without query parameters (scheme + host + path)
fn get_base_url(url: &str) -> Option<String> {
    let normalized = normalize_app_url(url.to_string());
    let parsed = Url::parse(&normalized).ok()?;
    let mut base = format!("{}://{}", parsed.scheme(), parsed.host_str()?);
    let path = parsed.path();
    if path != "/" {
        base.push_str(path);
    }
    Some(base)
}

/// Extracts just the domain (host) from a URL
fn get_domain(url: &str) -> Option<String> {
    let normalized = normalize_app_url(url.to_string());
    let parsed = Url::parse(&normalized).ok()?;
    parsed.host_str().map(|h| h.to_string())
}

/// Generates a URL-friendly slug from a name
fn generate_slug(name: &str) -> String {
    let slug_text = slug::slugify(name);
    // Ensure slug isn't empty
    if slug_text.is_empty() {
        "world".to_string()
    } else {
        slug_text
    }
}

/// Generates a unique slug, appending numbers if needed
async fn generate_unique_slug(data: &Data<AppState>, name: &str) -> String {
    let base_slug = generate_slug(name);

    // Check if base slug is available
    if let Ok(false) = slug_exists(data, &base_slug).await {
        return base_slug;
    }

    // Try appending numbers until we find a unique one
    for i in 2..1000 {
        let numbered_slug = format!("{}-{}", base_slug, i);
        if let Ok(false) = slug_exists(data, &numbered_slug).await {
            return numbered_slug;
        }
    }

    // Fallback: append random suffix
    format!("{}-{}", base_slug, rand::random::<u32>())
}

/// Generates a random verification code
fn generate_verification_code() -> String {
    let mut rng = rand::thread_rng();
    let code: String = (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect();
    code
}
