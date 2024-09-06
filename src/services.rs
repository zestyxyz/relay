use std::str::FromStr;

use activitypub_federation::actix_web::inbox::receive_activity;
use activitypub_federation::config::Data;
use activitypub_federation::fetch::object_id::ObjectId;
use activitypub_federation::fetch::webfinger::{build_webfinger_response, extract_webfinger_name};
use activitypub_federation::kinds::activity::CreateType;
use activitypub_federation::kinds::actor::ServiceType;
use activitypub_federation::protocol::context::WithContext;
use activitypub_federation::traits::{ActivityHandler, Actor, Object};
use activitypub_federation::FEDERATION_CONTENT_TYPE;
use actix_web::web::{self, Bytes};
use actix_web::{get, post, put, HttpRequest, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use sqlx::{self, FromRow};
use url::Url;

use crate::activities::{Create, DbActivity, Follow};
use crate::actors::{DbRelay, Relay};
use crate::apps::DbApp;
use crate::db::get_system_user;
use crate::error::Error;
use crate::AppState;

#[derive(Deserialize)]
pub struct BeaconPayload {
    pub url: String,
    pub name: String,
    pub description: String,
    pub active: bool,
}

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

#[get("/relay/beacon/{id}")]
async fn get_beacon(info: web::Path<i32>, data: Data<AppState>) -> impl Responder {
    match sqlx::query_as::<_, DbApp>("SELECT * FROM apps WHERE id = $1")
        .bind(info.into_inner() + 1)
        .fetch_one(&data.db)
        .await
    {
        Ok(app) => HttpResponse::Ok()
            .content_type(FEDERATION_CONTENT_TYPE)
            .json(app),
        Err(e) => {
            println!("Error fetching app from DB: {}", e);
            HttpResponse::NotFound().body("No beacon found")
        }
    }
}

#[put("/beacon")]
async fn new_beacon(data: Data<AppState>, req_body: web::Json<BeaconPayload>) -> impl Responder {
    let url = req_body.url.clone();
    let name = req_body.name.clone();
    let description = req_body.description.clone();
    let active = req_body.active;
    let system_user = get_system_user(&data).await.unwrap();
    let domain = system_user.ap_id.inner().as_str();
    let apps_count: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM apps")
        .fetch_one(&data.db)
        .await
    {
        Ok(count) => count,
        Err(e) => panic!("Error fetching apps count: {}", e),
    };
    let activities_count: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM activities")
        .fetch_one(&data.db)
        .await
    {
        Ok(count) => count,
        Err(e) => panic!("Error fetching apps count: {}", e),
    };
    match sqlx::query("INSERT INTO apps (activitypub_id, url, name, description, is_active) VALUES ($1, $2, $3, $4, $5)")
        .bind(format!("{}/beacon/{}", domain, apps_count))
        .bind(url)
        .bind(name)
        .bind(description)
        .bind(active)
        .execute(&data.db)
        .await {
            Ok(_) => (),
            Err(e) => println!("Error inserting new beacon: {}", e),
        };
    let activity = Create {
        actor: ObjectId::parse(domain).unwrap(),
        object: ObjectId::parse(&format!("{}/beacon/{}", domain, apps_count)).unwrap(),
        kind: CreateType::Create,
        id: Url::from_str(&format!("{}/activities/{}", domain, activities_count)).unwrap(),
    };
    let recipients: Vec<DbRelay> =
        match sqlx::query_as("SELECT f.* FROM followers f WHERE f.relay_id = 0")
            .fetch_all(&data.db)
            .await
        {
            Ok(relays) => relays,
            Err(e) => panic!("Error fetching relays: {}", e),
        };
    let recipient_inboxes: Vec<Url> = recipients.iter().map(|relay| relay.inbox.clone()).collect();
    let _ = system_user
        .send(activity, recipient_inboxes, false, &data)
        .await
        .map_err(|e| println!("Error sending activity: {}", e));
    HttpResponse::Ok()
}

#[get("/experiences")]
async fn get_experiences(data: Data<AppState>) -> impl Responder {
    match sqlx::query_as::<_, DbApp>("SELECT * FROM apps")
        .fetch_all(&data.db)
        .await
    {
        Ok(apps) => HttpResponse::Ok().json(apps),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[get("/relays")]
async fn get_relays(data: Data<AppState>) -> impl Responder {
    match sqlx::query_as::<_, DbRelay>("SELECT * FROM relays")
        .fetch_all(&data.db)
        .await
    {
        Ok(relays) => HttpResponse::Ok().json(relays),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

/// Handles requests to fetch system user json over HTTP
#[get("/relay")]
async fn http_get_system_user(data: Data<AppState>) -> impl Responder {
    println!("Got a request for the system user");
    let user = sqlx::query_as::<_, DbRelay>("SELECT * FROM relays WHERE id = $1")
        .bind(0)
        .fetch_one(&data.db)
        .await
        .unwrap();
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
    match sqlx::query_as::<_, DbActivity>("SELECT * FROM activities WHERE id = $1")
        .bind(info.into_inner())
        .fetch_one(&data.db)
        .await
    {
        Ok(activity) => HttpResponse::Ok()
            .content_type(FEDERATION_CONTENT_TYPE)
            .json(activity),
        Err(_) => HttpResponse::NotFound().body("No activity found"),
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
#[enum_delegate::implement(ActivityHandler)]
pub enum RelayAcceptedActivities {
    Follow(Follow),
    Create(Create),
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

pub async fn not_found(request: HttpRequest) -> impl Responder {
    println!("Got request for: {}", request.uri().path());
    HttpResponse::NotFound()
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
            println!("{:?}", e);
            "bad"
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

#[get("/test_follow")]
async fn test_follow(data: Data<AppState>) -> impl Responder {
    let db_user = get_system_user(&data).await.unwrap();
    let port = std::env::var("PORT").expect("PORT must be set");
    let port = u16::from_str(&port).unwrap();
    let port = if port == 8000 { 8001 } else { 8000 };
    match db_user
        .follow(&format!("relay@localhost:{}", port), &data)
        .await
    {
        Ok(_) => HttpResponse::Ok().body("Followed"),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}
