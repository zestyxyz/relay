mod activities;
mod actors;
mod error;
mod experiences;
mod schemas;
mod temp;
use temp::{PRIVATE_KEY, PUBLIC_KEY};

use std::env;
use std::str::FromStr;

use activities::{Create, Follow};
use activitypub_federation::actix_web::inbox::receive_activity;
use activitypub_federation::config::{Data, FederationConfig, FederationMiddleware};
use activitypub_federation::fetch::object_id::ObjectId;
use activitypub_federation::http_signatures::generate_actor_keypair;
use activitypub_federation::kinds::activity::CreateType;
use activitypub_federation::kinds::actor::ServiceType;
use activitypub_federation::protocol::context::WithContext;
use activitypub_federation::protocol::public_key::PublicKey;
use activitypub_federation::traits::ActivityHandler;
use activitypub_federation::FEDERATION_CONTENT_TYPE;
use actix_cors::Cors;
use actix_web::web::Bytes;
use actix_web::{get, post, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actors::{DbRelay, Relay};
use getopts::Options;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::experiences::DbExperience;
use crate::schemas::{BeaconPayload, RelayPayload};

// Temporary fixture to avoid dealing with database adapaters
static mut EXPERIENCE_LIST: Vec<DbExperience> = Vec::new();
static mut RELAYS: Vec<DbRelay> = Vec::new();
static mut SYSTEM_USER: Option<DbRelay> = None;

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

#[get("/beacon/{id}")]
async fn get_beacon() -> impl Responder {
    let experience = unsafe {
        EXPERIENCE_LIST
            .get(0)
            .unwrap_or_else(|| panic!("No beacon found"))
    };
    HttpResponse::Ok().json(experience)
}

#[put("/beacon")]
async fn new_beacon(data: Data<()>, req_body: web::Json<BeaconPayload>) -> impl Responder {
    let url = req_body.url.clone();
    let name = req_body.name.clone();
    let description = req_body.description.clone();
    let active = req_body.active;
    unsafe {
        EXPERIENCE_LIST.push(DbExperience::new(
            ObjectId::parse("http://localhost:8000/beacon/0").unwrap(),
            url,
            name,
            description,
            active,
        ));
    }
    let activity = Create {
        actor: ObjectId::parse("http://localhost:8000/relay").unwrap(),
        object: ObjectId::parse("http://localhost:8000/beacon/0").unwrap(),
        kind: CreateType::Create,
        id: Url::from_str("http://localhost:8000/activity/0").unwrap(),
    };
    unsafe {
        let recipient = RELAYS[0].inbox.clone();
        let _ = SYSTEM_USER
            .as_ref()
            .unwrap()
            .send(activity, vec![recipient], false, &data)
            .await
            .map_err(|e| println!("Error sending activity: {}", e));
    }
    HttpResponse::Ok()
}

#[get("/experiences")]
async fn get_experiences() -> impl Responder {
    HttpResponse::Ok().json(unsafe { EXPERIENCE_LIST.clone() })
}

/// Handles requests to fetch system user json over HTTP
#[get("/relay")]
async fn http_get_system_user(_data: Data<()>) -> impl Responder {
    println!("Got a request for the system user");
    let json_user = Relay {
        id: ObjectId::parse("http://localhost:8000/relay").unwrap(),
        kind: ServiceType::Service,
        preferred_username: String::new(),
        name: String::new(),
        inbox: Url::from_str("http://localhost:8000/relay/inbox").unwrap(),
        outbox: Url::from_str("http://localhost:8000/relay/outbox").unwrap(),
        public_key: PublicKey {
            id: "".to_string(),
            owner: Url::from_str("http://localhost:8000/relay").unwrap(),
            public_key_pem: PUBLIC_KEY.to_string(),
        },
    };
    HttpResponse::Ok()
        .content_type(FEDERATION_CONTENT_TYPE)
        .json(WithContext::new_default(json_user))
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

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args: Vec<String> = env::args().collect();
    let mut opts = Options::new();
    opts.optopt("p", "port", "Port to run server on", "");
    let matches = opts.parse(args).unwrap();
    let port = matches.opt_get("p").unwrap_or(Some(8000)).unwrap();
    let other_port = if port == 8000 { 8001 } else { 8000 };

    unsafe {
        let keypair = generate_actor_keypair().unwrap();
        let relay = DbRelay::new(
            "Test Relay 1".to_string(),
            ObjectId::parse(&format!("http://localhost:{}/relay", port)).unwrap(),
            Url::from_str(&format!("http://localhost:{}/relay/inbox", port)).unwrap(),
            PUBLIC_KEY.to_string(),
            Some(PRIVATE_KEY.to_string()),
            true,
        );
        SYSTEM_USER = Some(relay);
        let relay2 = DbRelay::new(
            "Test Relay 2".to_string(),
            ObjectId::parse(&format!("http://localhost:{}/relay", other_port)).unwrap(),
            Url::from_str(&format!("http://localhost:{}/relay/inbox", other_port)).unwrap(),
            PUBLIC_KEY.to_string(),
            None,
            false,
        );
        RELAYS.push(relay2);
    }

    let config = FederationConfig::builder()
        .domain("localhost")
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
            .service(get_experiences)
            .default_service(web::to(not_found))
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await;
    Ok(())
}
