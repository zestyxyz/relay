use std::collections::HashMap;
use std::str::FromStr;

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
use jwt_simple::prelude::*;
use serde::{Deserialize, Serialize};
use tera::Context;
use url::Url;

use super::activities::{Create, Follow, Update};
use super::actors::{DbRelay, Relay};
use super::apps::App;
use super::db::{
    create_activity, create_app, get_activities_count, get_activity_by_id, get_all_apps,
    get_all_relays, get_app_by_id, get_app_by_url, get_apps_count, get_relay_by_id,
    get_relay_followers, get_system_user, update_app,
};
use crate::AppState;

#[derive(Deserialize)]
pub struct BeaconPayload {
    pub url: String,
    pub name: String,
    pub description: String,
    pub active: bool,
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

#[get("/")]
async fn index(data: Data<AppState>) -> impl Responder {
    let count = get_apps_count(&data).await.unwrap();
    match get_all_apps(&data).await {
        Ok(mut apps) => {
            apps.truncate(20);
            let mut ctx = tera::Context::new();
            ctx.insert("apps_count", &count);
            ctx.insert("apps", &apps);
            match data.tera.render("index.html", &ctx) {
                Ok(html) => web::Html::new(html),
                Err(e) => {
                    println!("{}", e);
                    web::Html::new("Failed to render to template!")
                }
            }
        }
        Err(e) => {
            println!("{}", e);
            web::Html::new("Failed to render to template!")
        }
    }
}

#[get("/relay/beacon/{id}")]
async fn get_beacon(info: web::Path<i32>, data: Data<AppState>) -> impl Responder {
    match get_app_by_id(info.into_inner() + 1, &data).await {
        Ok(app) => HttpResponse::Ok()
            .content_type(FEDERATION_CONTENT_TYPE)
            .json(App::new(
                app.ap_id,
                String::new(),
                vec![],
                app.url,
                app.name,
                app.description,
            )),
        Err(e) => {
            println!("Error fetching app from DB: {}", e);
            HttpResponse::NotFound().body("No beacon found")
        }
    }
}

#[put("/beacon")]
async fn new_beacon(data: Data<AppState>, req_body: web::Json<BeaconPayload>) -> impl Responder {
    // Extract fields from request body
    let url = req_body.url.clone();
    let name = req_body.name.clone();
    let description = req_body.description.clone();
    let active = req_body.active;

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

    // Check if app already exists, if so return 304
    // TODO: Improve readability of this block
    match get_app_by_url(&data, &url).await {
        Ok(Some(app)) => {
            if app.name != name || app.description != description || app.active != active {
                match update_app(
                    &data,
                    url.clone(),
                    name.clone(),
                    description.clone(),
                    active,
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
                                let recipients: Vec<DbRelay> =
                                    match get_relay_followers(&data).await {
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
            } else {
                return HttpResponse::NotModified().finish();
            }
        }
        Ok(None) => {
            println!("We didn't find the app, we should be creating it");
        }
        Err(e) => println!("Error fetching app from DB: {}", e),
    }

    // Create new app and send create activity to following relays
    let ap_id = format!("{}/beacon/{}", domain, apps_count);
    match create_app(&data, ap_id, url, name, description, active).await {
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
    match get_app_by_id(path.into_inner() + 1, &data).await {
        Ok(app) => {
            let mut ctx = tera::Context::new();
            ctx.insert("name", &app.name);
            ctx.insert("description", &app.description);
            ctx.insert("url", &app.url);
            match data.tera.render("app.html", &ctx) {
                Ok(html) => web::Html::new(html),
                Err(e) => {
                    println!("{}", e);
                    web::Html::new("Failed to render to template!")
                }
            }
        }
        Err(e) => {
            println!("Error fetching app from DB: {}", e);
            match data.tera.render("error.html", &Context::new()) {
                Ok(html) => web::Html::new(html),
                Err(e) => {
                    println!("{}", e);
                    web::Html::new("Failed to render to template!")
                }
            }
        }
    }
}

#[get("/apps")]
async fn get_apps(data: Data<AppState>) -> impl Responder {
    match get_all_apps(&data).await {
        Ok(apps) => {
            // TODO: See if calculating this can be lifted off a hot path
            let mut host_occurances: HashMap<String, usize> = HashMap::new();
            apps.iter().for_each(|app| {
                let url = Url::parse(&app.url).unwrap();
                let host = url.host_str();
                if let Some(hostname) = host {
                    let _ = host_occurances.entry(hostname.to_string()).and_modify(|count| *count += 1).or_insert(0);
                }
            });
            let high_occurances: Vec<String> = host_occurances
                .into_iter()
                .filter_map(|(host, count)| {
                    if count > 3 {
                        Some(host)
                    } else {
                        None
                    }
                })
                .collect();
            let mut ctx = tera::Context::new();
            ctx.insert("apps", &apps);
            ctx.insert("DEBUG", &data.debug);
            ctx.insert("high_occurances", &high_occurances);
            match data.tera.render("apps.html", &ctx) {
                Ok(html) => web::Html::new(html),
                Err(e) => {
                    println!("{}", e);
                    web::Html::new("Failed to render to template!")
                }
            }
        }
        Err(e) => {
            println!("Error fetching apps from DB: {}", e);
            match data.tera.render("error.html", &Context::new()) {
                Ok(html) => web::Html::new(html),
                Err(e) => {
                    println!("{}", e);
                    web::Html::new("Failed to render to template!")
                }
            }
        }
    }
}

#[get("/relays")]
async fn get_relays(data: Data<AppState>) -> impl Responder {
    match get_all_relays(&data).await {
        Ok(relays) => {
            let mut ctx = tera::Context::new();
            ctx.insert("relays", &relays);
            match data.tera.render("relays.html", &ctx) {
                Ok(html) => web::Html::new(html),
                Err(e) => {
                    println!("{}", e);
                    web::Html::new("Failed to render to template!")
                }
            }
        }
        Err(e) => {
            println!("Error fetching apps from DB: {}", e);
            match data.tera.render("error.html", &Context::new()) {
                Ok(html) => web::Html::new(html),
                Err(e) => {
                    println!("{}", e);
                    web::Html::new("Failed to render to template!")
                }
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
    match data.tera.render("error.html", &Context::new()) {
        Ok(html) => web::Html::new(html),
        Err(e) => {
            println!("{}", e);
            web::Html::new("Failed to render to template!")
        }
    }
}

#[get("/login")]
async fn login(_request: HttpRequest, data: Data<AppState>) -> impl Responder {
    match data.tera.render("login.html", &Context::new()) {
        Ok(html) => web::Html::new(html),
        Err(e) => {
            println!("{}", e);
            web::Html::new("Failed to render to template!")
        }
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

#[get("/admin")]
async fn admin_page(request: HttpRequest, data: Data<AppState>) -> impl Responder {
    let cookie = request.cookie("relay-admin-token");
    if cookie.is_none() {
        return match data.tera.render("error.html", &Context::new()) {
            Ok(html) => web::Html::new(html),
            Err(e) => {
                println!("{}", e);
                web::Html::new("Failed to render to template!")
            }
        };
    }
    let mut ctx = tera::Context::new();
    ctx.insert("message", "");
    match data.tera.render("admin.html", &ctx) {
        Ok(html) => web::Html::new(html),
        Err(e) => {
            println!("{}", e);
            web::Html::new("Failed to render to template!")
        }
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
    match db_user.follow(&req_body.follow_url, &data).await {
        Ok(_) => match data.tera.render("admin.html", &ctx) {
            Ok(html) => HttpResponse::Ok().body(html),
            Err(e) => {
                println!("{}", e);
                HttpResponse::InternalServerError().body(e.to_string())
            }
        },
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}
