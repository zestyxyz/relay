use std::collections::HashMap;
use std::env;
use std::str::FromStr;

extern crate rand;
use rand::seq::SliceRandom;
use rand::thread_rng;

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
    get_all_relays, get_app_by_id, get_app_by_url, get_apps_count, get_relay_by_id,
    get_relay_followers, get_system_user, toggle_app_visibility, update_app,
};
use crate::{AppState, SessionInfo};

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

#[derive(Serialize)]
pub struct JWT {
    pub token: String,
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
    println!("{}", e);
    web::Html::new("Failed to render to template!")
}

fn server_fail_screen(e: super::error::Error) -> web::Html {
    println!("{}", e);
    web::Html::new("Server has encountered an internal error. Please check again later.")
}

#[get("/")]
async fn index(data: Data<AppState>) -> impl Responder {
    let template_path = get_template_path(&data, "index");
    match get_all_apps(&data).await {
        Ok(mut apps) => {
            // Filter apps for display in the front carousel
            if !data.debug {
                apps.retain(|app| !app.url.contains("localhost"));
            }
            apps.retain(|app| app.visible);
            let total_apps_count = apps.len();
            let mut unique_urls = HashSet::new();
            apps.retain(|app| {
                let url = Url::parse(&app.url)
                    .expect(format!("This app is holding an invalid URL: {}", app.url).as_str());
                unique_urls.insert(url.host_str().unwrap().to_string())
            });

            // Show Top 20
            apps.truncate(20);

            let mut shuffled_apps = apps.to_vec();
            shuffled_apps.shuffle(&mut thread_rng());

            // Get live counts
            let mut live_counts = vec![];
            prune_old_sessions(&data);
            let sessions = match data.sessions.read() {
                Ok(sessions) => sessions,
                Err(poisoned) => {
                    println!("Warning: sessions lock was poisoned. Attempting recovery...");
                    poisoned.into_inner()
                }
            };
            for app in shuffled_apps.iter_mut() {
                let live_count: usize = sessions
                    .get(&app.url)
                    .map(|sessions| sessions.len())
                    .unwrap_or(0);
                live_counts.push(live_count);
            }

            // Render
            let mut ctx = tera::Context::new();
            ctx.insert("apps_count", &total_apps_count);
            ctx.insert("apps", &apps);
            ctx.insert("shuffled_apps", &shuffled_apps);
            ctx.insert("live_counts", &live_counts);

            match data.tera.render(&template_path, &ctx) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
        Err(e) => server_fail_screen(e),
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
            println!("Error fetching app from DB: {}", e);
            HttpResponse::NotFound().body("No beacon found")
        }
    }
}

#[put("/beacon")]
async fn new_beacon(data: Data<AppState>, req_body: web::Json<BeaconPayload>) -> impl Responder {
    // Env vars
    let relay_domain = env::var("DOMAIN").expect("DOMAIN must be set");
    let protocol = env::var("PROTOCOL").expect("PROTOCOL must be set");

    // Extract fields from request body
    let url = req_body.url.clone();
    let name = req_body.name.clone();
    let description = req_body.description.clone();
    let active = req_body.active;
    let image = req_body.image.clone().unwrap_or("#".to_string());
    let adult = req_body.adult.unwrap_or(false);
    let tags = req_body.tags.clone().unwrap_or("".to_string());

    // Query system user and DB information
    let system_user = get_system_user(&data).await.unwrap();
    let domain = system_user.ap_id.inner().as_str();
    let apps_count = match get_apps_count(&data).await {
        Ok(count) => count,
        Err(e) => panic!("Error fetching apps count: {}", e),
    };
    let activities_count: i64 = match get_activities_count(&data).await {
        Ok(count) => count,
        Err(e) => panic!("Error fetching activities count: {}", e),
    };

    // Check if app already exists.
    // If it does and nothing changed, return 304
    // Otherwise, update the DB and send the relevant activities
    match get_app_by_url(&data, &url).await {
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
                    &app.ap_id.clone().into_inner().as_str(),
                    &protocol,
                    &relay_domain,
                    app_image,
                );
                if image_url.is_empty() {
                    println!("Error creating local image");
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
                                Err(e) => panic!("Error fetching relays: {}", e),
                            };
                            let recipient_inboxes: Vec<Url> =
                                recipients.iter().map(|relay| relay.inbox.clone()).collect();
                            let _ = system_user
                                .send(activity, recipient_inboxes, false, &data)
                                .await
                                .map_err(|e| println!("Error sending activity: {}", e));
                        }
                        Err(e) => {
                            println!("ERROR CREATING ACTIVITY: {}", e.to_string());
                            return HttpResponse::InternalServerError().body(e.to_string());
                        }
                    }

                    return HttpResponse::Ok().finish();
                }
                Err(e) => println!("Error updating app: {}", e),
            }
        }
        Ok(None) => {
            println!("We didn't find the app, we should be creating it");
        }
        Err(e) => println!("Error fetching app from DB: {}", e),
    }

    // At this point, it should be certain that the app doesn't already exist.
    // Create a new app and send the Create activity to following relays
    let ap_id = format!("{}/beacon/{}", domain, apps_count);
    let image_url = if image.contains("data:") {
        let image_url = create_local_image(&ap_id, &protocol, &relay_domain, &image);
        if image_url.is_empty() {
            println!("Error creating local image");
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
        name,
        description,
        active,
        image_url,
        adult,
        tags.clone(),
    )
    .await
    {
        Ok(_) => (),
        Err(e) => println!("Error inserting new beacon: {}", e),
    };
    let activity = Create {
        actor: ObjectId::parse(domain).unwrap(),
        object: ObjectId::parse(&format!("{}/beacon/{}", domain, apps_count)).unwrap(),
        kind: CreateType::Create,
        id: Url::from_str(&format!("{}/activities/{}", domain, activities_count)).unwrap(),
    };
    let recipients: Vec<DbRelay> = match get_relay_followers(&data).await {
        Ok(relays) => relays,
        Err(e) => panic!("Error fetching relays: {}", e),
    };
    let recipient_inboxes: Vec<Url> = recipients.iter().map(|relay| relay.inbox.clone()).collect();
    let _ = system_user
        .send(activity, recipient_inboxes, false, &data)
        .await
        .map_err(|e| println!("Error sending activity: {}", e));

    HttpResponse::Ok().finish()
}

#[get("/app/{id}")]
async fn get_app(data: Data<AppState>, path: web::Path<i32>) -> impl Responder {
    let template_path = get_template_path(&data, "app");
    let error_path = get_template_path(&data, "error");
    match get_app_by_id(path.into_inner() + 1, &data).await {
        Ok(app) => {
            prune_old_sessions(&data);
            let sessions = match data.sessions.read() {
                Ok(sessions) => sessions,
                Err(poisoned) => {
                    println!("Warning: sessions lock was poisoned. Attempting recovery...");
                    poisoned.into_inner()
                }
            };
            let live_count = sessions
                .get(&app.url)
                .map(|sessions| sessions.len())
                .unwrap_or(0);
            let mut ctx = tera::Context::new();
            ctx.insert("name", &app.name);
            ctx.insert("description", &app.description);
            ctx.insert("url", &app.url);
            ctx.insert("image", &app.image);
            ctx.insert("live_count", &live_count);
            match data.tera.render(&template_path, &ctx) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
        Err(e) => {
            println!("Error fetching app from DB: {}", e);
            match data.tera.render(&error_path, &Context::new()) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
    }
}

#[get("/apps")]
async fn get_apps(data: Data<AppState>) -> impl Responder {
    let template_path = get_template_path(&data, "apps");
    let error_path = get_template_path(&data, "error");
    match get_all_apps(&data).await {
        Ok(apps) => {
            // TODO: See if calculating this can be lifted off a hot path
            let mut app_groups: HashMap<String, Vec<DbApp>> = HashMap::new();
            let mut app_page_urls: HashMap<String, String> = HashMap::new();
            apps.iter().for_each(|app| {
                app_groups
                    .entry(app.name.clone())
                    .and_modify(|entries| entries.push(app.clone()))
                    .or_insert(vec![app.clone()]);
                app_page_urls.insert(app.url.clone(), app.page_url());
            });
            let mut ctx = tera::Context::new();
            ctx.insert("apps", &app_groups.values().collect::<Vec<_>>());
            ctx.insert("app_pages", &app_page_urls);
            ctx.insert("DEBUG", &data.debug);
            ctx.insert("SHOW_ADULT_CONTENT", &data.show_adult_content);
            match data.tera.render(&template_path, &ctx) {
                Ok(html) => web::Html::new(html),
                Err(e) => template_fail_screen(e),
            }
        }
        Err(e) => {
            println!("Error fetching apps from DB: {}", e);
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
            println!("Error fetching apps from DB: {}", e);
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
            println!("Error fetching activity: {}", e);
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

    let duration = Duration::from_days(1);
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
    let image_url = format!("images/{}", request.match_info().get("id").unwrap());
    let image = match std::fs::read(&image_url) {
        Ok(image_bytes) => image_bytes,
        Err(_) => {
            println!("Failed to load image at: {}", image_url);
            std::fs::read("frontend/images/noimage.png").expect("Failed to load placeholder image")
        }
    };
    let mime = match image_url.split('.').last() {
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
    let cookie = request.cookie("relay-admin-token");
    if cookie.is_none() {
        return HttpResponse::TemporaryRedirect()
        .append_header(("Location", "/login"))
        .finish();
    }
    let mut ctx = tera::Context::new();
    ctx.insert("message", "");
    match get_all_apps(&data).await {
        Ok(apps) => {
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
            println!("Error during webfinger lookup: {:?}", e);
            ""
        }
    };
    if name != "relay" {
        return HttpResponse::NotFound().finish();
    }
    let db_user = get_system_user(&data).await.unwrap();
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
    let mut sessions = match data.sessions.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            println!("Warning: sessions lock was poisoned. Attempting recovery...");
            poisoned.into_inner()
        }
    };
    match sessions.get_mut(&req_body.url) {
        Some(vec) => {
            match vec
                .iter_mut()
                .find(|info| info.session_id == req_body.session_id)
            {
                Some(session) => session.timestamp = req_body.timestamp,
                None => vec.push(session_info),
            }
        }
        None => {
            let mut vec = Vec::new();
            vec.push(session_info);
            sessions.insert(req_body.url.clone(), vec);
        }
    }

    HttpResponse::Ok().finish()
}

#[post("/admin/follow")]
async fn admin_follow(
    request: HttpRequest,
    req_body: web::Form<FollowPayload>,
    data: Data<AppState>,
) -> HttpResponse {
    let cookie = request.cookie("relay-admin-token");
    if cookie.is_none() {
        return HttpResponse::InternalServerError().body("Authorization error occurred.");
    }
    let db_user = get_system_user(&data).await.unwrap();
    let mut ctx = tera::Context::new();
    ctx.insert("message", "Successfully followed!");
    let template_path = get_template_path(&data, "admin");
    match db_user.follow(&req_body.follow_url, &data).await {
        Ok(_) => match data.tera.render(&template_path, &ctx) {
            Ok(html) => HttpResponse::Ok().body(html),
            Err(e) => {
                println!("{}", e);
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
    let cookie = request.cookie("relay-admin-token");
    if cookie.is_none() {
        return HttpResponse::InternalServerError().body("Authorization error occurred.");
    }
    match toggle_app_visibility(req_body.app_id, &data).await {
        Ok(_) => {
            let mut ctx = tera::Context::new();
            ctx.insert("message", "Visiblity toggled!");
            let template_path = get_template_path(&data, "admin");
            match data.tera.render(&template_path, &ctx) {
                Ok(html) => HttpResponse::Ok().body(html),
                Err(e) => {
                    println!("{}, {}", e, ctx.into_json());
                    return HttpResponse::InternalServerError().body(e.to_string());
                }
            }
        }
        Err(e) => {
            HttpResponse::InternalServerError().body(e.to_string())
        },
    }
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
            println!("Error parsing image data: {:?}", e);
            return "".to_string();
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
            println!("Warning: sessions lock was poisoned during pruning. Attempting recovery...");
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
