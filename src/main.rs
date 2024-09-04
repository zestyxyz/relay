mod activities;
mod actors;
mod error;
mod apps;
mod schemas;
mod temp;

use std::cell::Cell;
use std::env;
use std::str::FromStr;

use activities::{Create, Follow};
use activitypub_federation::actix_web::inbox::receive_activity;
use activitypub_federation::config::{Data, FederationConfig, FederationMiddleware};
use activitypub_federation::fetch::object_id::ObjectId;
use activitypub_federation::fetch::webfinger::{build_webfinger_response, extract_webfinger_name};
use activitypub_federation::kinds::activity::CreateType;
use activitypub_federation::kinds::actor::ServiceType;
use activitypub_federation::protocol::context::WithContext;
use activitypub_federation::traits::{ActivityHandler, Actor, Object};
use activitypub_federation::FEDERATION_CONTENT_TYPE;
use actix_cors::Cors;
use actix_web::web::Bytes;
use actix_web::{get, post, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actors::{DbRelay, Relay};
use getopts::Options;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::apps::DbApp;
use crate::error::Error;
use crate::schemas::BeaconPayload;
use crate::temp::{PRIVATE_KEY, PUBLIC_KEY};

// Temporary fixture to avoid dealing with database adapaters
static mut APPS_LIST: Vec<DbApp> = Vec::new();
static mut RELAYS: Vec<DbRelay> = Vec::new();
static mut SYSTEM_USER: Option<DbRelay> = None;
static mut ACTIVITIES: Vec<RelayAcceptedActivities> = Vec::new();
static mut PORT: Cell<u16> = Cell::new(8000);

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

#[get("/beacon/{id}")]
async fn get_beacon(data: Data<()>) -> impl Responder {
    let experience = unsafe {
        APPS_LIST
            .first()
            .unwrap_or_else(|| panic!("No beacon found"))
            .clone()
            .into_json(&data)
            .await
            .unwrap()
    };
    HttpResponse::Ok()
        .content_type(FEDERATION_CONTENT_TYPE)
        .json(experience)
}

#[put("/beacon")]
async fn new_beacon(data: Data<()>, req_body: web::Json<BeaconPayload>) -> impl Responder {
    let url = req_body.url.clone();
    let name = req_body.name.clone();
    let description = req_body.description.clone();
    let active = req_body.active;
    let apps_count = unsafe { APPS_LIST.len() + 1 };
    let activities_count = unsafe { ACTIVITIES.len() + 1 };
    let domain = unsafe { SYSTEM_USER.clone().unwrap().ap_id.inner().to_string() };
    unsafe {
        APPS_LIST.push(DbApp::new(
            ObjectId::parse(&format!("{}/beacon/{}", domain, apps_count)).unwrap(),
            url,
            name,
            description,
            active,
        ));
    }
    let activity = Create {
        actor: ObjectId::parse(&format!("{}/relay", domain)).unwrap(),
        object: ObjectId::parse(&format!("{}/beacon/{}", domain, apps_count)).unwrap(),
        kind: CreateType::Create,
        id: Url::from_str(&format!("{}/activity/{}", domain, activities_count)).unwrap(),
    };
    unsafe {
        let recipients: Vec<Url> = RELAYS.iter().map(|relay| relay.inbox.clone()).collect();
        let _ = SYSTEM_USER
            .as_ref()
            .unwrap()
            .send(activity, recipients, false, &data)
            .await
            .map_err(|e| println!("Error sending activity: {}", e));
    }
    HttpResponse::Ok()
}

#[get("/experiences")]
async fn get_experiences() -> impl Responder {
    HttpResponse::Ok().json(unsafe { APPS_LIST.clone() })
}

#[get("/relays")]
async fn get_relays() -> impl Responder {
    HttpResponse::Ok().json(unsafe { RELAYS.clone() })
}

/// Handles requests to fetch system user json over HTTP
#[get("/relay")]
async fn http_get_system_user(_data: Data<()>) -> impl Responder {
    println!("Got a request for the system user");
    let user = unsafe { SYSTEM_USER.clone().unwrap() };
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
async fn get_activity(data: Data<()>) -> impl Responder {
    let activity = unsafe {
        ACTIVITIES
            .first()
            .unwrap_or_else(|| panic!("No activity found"))
    };
    HttpResponse::Ok()
        .content_type(FEDERATION_CONTENT_TYPE)
        .json(activity)
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
#[enum_delegate::implement(ActivityHandler)]
pub enum RelayAcceptedActivities {
    Follow(Follow),
    Create(Create),
}

#[post("/relay/inbox")]
async fn http_post_relay_inbox(request: HttpRequest, body: Bytes, data: Data<()>) -> HttpResponse {
    println!("Got a request to the relay inbox");
    match receive_activity::<WithContext<RelayAcceptedActivities>, DbRelay, ()>(
        request, body, &data,
    )
    .await
    {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

async fn not_found(request: HttpRequest) -> impl Responder {
    println!("Got request for: {}", request.uri().path());
    HttpResponse::NotFound()
}

#[derive(Deserialize)]
pub struct WebfingerQuery {
    resource: String,
}

#[get("/.well-known/webfinger")]
async fn webfinger(
    query: web::Query<WebfingerQuery>,
    data: Data<()>,
) -> impl Responder {
    let name = match extract_webfinger_name(&query.resource, &data) {
        Ok(name) => name,
        Err(e) => {
            println!("{:?}", e);
            "bad"
        },
    };
    if name != "relay" {
        return HttpResponse::NotFound().finish();
    }
    let db_user = unsafe { SYSTEM_USER.clone().unwrap() };
    HttpResponse::Ok().json(build_webfinger_response(
        query.resource.clone(),
        db_user.ap_id.into_inner(),
    ))
}

#[get("/test_follow")]
async fn test_follow(data: Data<()>) -> impl Responder {
    let db_user = unsafe { SYSTEM_USER.clone().unwrap() };
    let port = unsafe { PORT.get() };
    let port = if port == 8000 { 8001 } else { 8000 };
    match db_user
        .follow(&format!("relay@localhost:{}", port), &data)
        .await {
            Ok(_) => HttpResponse::Ok().body("Followed"),
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
        }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args: Vec<String> = env::args().collect();
    let mut opts = Options::new();
    opts.optopt("p", "port", "Port to run server on", "");
    let matches = opts.parse(args).unwrap();
    let port = matches.opt_get("p").unwrap_or(Some(8000)).unwrap();
    unsafe { PORT.set(port); }

    unsafe {
        let relay = DbRelay::new(
            "relay".to_string(),
            ObjectId::parse(&format!("http://localhost:{}/relay", port)).unwrap(),
            Url::from_str(&format!("http://localhost:{}/relay/inbox", port)).unwrap(),
            Url::from_str(&format!("http://localhost:{}/relay/outbox", port)).unwrap(),
            PUBLIC_KEY.to_string(),
            Some(PRIVATE_KEY.to_string()),
            true,
        );
        SYSTEM_USER = Some(relay);
    }

    let config = FederationConfig::builder()
        .domain(format!("localhost:{}", port))
        .app_data(())
        .debug(true)
        .build()
        .await?;
    let _ = HttpServer::new(move || {
        let cors = Cors::permissive();
        println!("Server listening on: http://localhost:{}", port);
        App::new()
            .wrap(FederationMiddleware::new(config.clone()))
            .wrap(cors)
            .service(http_get_system_user)
            .service(http_post_relay_inbox)
            .service(new_beacon)
            .service(get_beacon)
            .service(get_activity)
            .service(get_experiences)
            .service(get_relays)
            .service(test_follow)
            .service(webfinger)
            .default_service(web::to(not_found))
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await;
    Ok(())
}
