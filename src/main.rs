mod activities;
mod actors;
mod experiences;
mod schemas;

use std::env;
use std::str::FromStr;

use activities::{Create, Follow};
use activitypub_federation::actix_web::inbox::receive_activity;
use activitypub_federation::config::{Data, FederationConfig, FederationMiddleware};
use activitypub_federation::fetch::object_id::ObjectId;
use activitypub_federation::kinds::actor::ServiceType;
use activitypub_federation::protocol::context::WithContext;
use activitypub_federation::protocol::public_key::PublicKey;
use activitypub_federation::traits::ActivityHandler;
use activitypub_federation::FEDERATION_CONTENT_TYPE;
use actix_cors::Cors;
use actix_web::web::Bytes;
use actix_web::{get, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actors::{DbRelay, Relay};
use getopts::Options;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::experiences::Experience;
use crate::schemas::{BeaconPayload, RelayPayload};

// Temporary fixture to avoid dealing with database adapaters
static mut EXPERIENCE_LIST: Vec<Experience> = Vec::new();
static mut RELAYS: Vec<String> = Vec::new();

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

#[put("/beacon")]
async fn beacon(req_body: web::Json<BeaconPayload>) -> impl Responder {
    let url = req_body.url.clone();
    let name = req_body.name.clone();
    let description = req_body.description.clone();
    let active = req_body.active;
    unsafe {
        EXPERIENCE_LIST.push(Experience::new(url, name, description, active));
    }
    HttpResponse::Ok()
}

#[put("/relay")]
async fn relay(req_body: web::Json<RelayPayload>) -> impl Responder {
    let url = req_body.url.clone();
    unsafe {
        RELAYS.push(url);
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
            public_key_pem: "".to_string(),
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

#[put("/relay/inbox")]
async fn http_post_relay_inbox(
    request: HttpRequest,
    body: Bytes,
    data: Data<()>,
) -> HttpResponse {
    match receive_activity::<WithContext<RelayAcceptedActivities>, DbRelay, ()>(
        request, body, &data,
    )
    .await
    {
        Ok(_) => HttpResponse::Ok().finish(),
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
    if port == 8000 {
        unsafe { RELAYS.push("http://localhost:8001".into()) }
    }
    let config = FederationConfig::builder()
        .domain("localhost")
        .app_data(())
        .build()
        .await?;
    let _ = HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(FederationMiddleware::new(config.clone()))
            .wrap(cors)
            .service(http_get_system_user)
            .service(http_post_relay_inbox)
            .service(beacon)
            .service(get_experiences)
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await;
    Ok(())
}
