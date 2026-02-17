use activitypub_federation::config::Data;
use sqlx::Row;

use super::activities::DbActivity;
use super::actors::DbRelay;
use super::apps::DbApp;
use super::error::Error;
use crate::AppState;

pub async fn get_system_user(data: &Data<AppState>) -> Result<DbRelay, Error> {
    let db = &data.db;
    let user = sqlx::query_as::<_, DbRelay>("SELECT * FROM relays WHERE id = 0 LIMIT 1")
        .fetch_one(db)
        .await?;
    Ok(user)
}

pub async fn get_app_by_id(id: i32, data: &Data<AppState>) -> Result<DbApp, Error> {
    let db = &data.db;
    let app = sqlx::query_as::<_, DbApp>("SELECT * FROM apps WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(app)
}

pub async fn get_app_by_ap_id(data: &Data<AppState>, ap_id: &str) -> Result<Option<DbApp>, Error> {
    let db = &data.db;
    let app = sqlx::query_as::<_, DbApp>("SELECT * FROM apps WHERE activitypub_id = $1")
        .bind(ap_id)
        .fetch_optional(db)
        .await?;
    Ok(app)
}

/// Find an app by base URL (ignoring query parameters)
/// Uses LIKE pattern matching: base_url% to match URLs with any query string
pub async fn get_app_by_base_url(data: &Data<AppState>, base_url: &str) -> Result<Option<DbApp>, Error> {
    let db = &data.db;
    // Match the base URL with or without query parameters
    let pattern = format!("{}%", base_url);
    let app = sqlx::query_as::<_, DbApp>("SELECT * FROM apps WHERE url LIKE $1 ORDER BY id ASC LIMIT 1")
        .bind(pattern)
        .fetch_optional(db)
        .await?;
    Ok(app)
}

pub async fn get_all_apps(data: &Data<AppState>) -> Result<Vec<DbApp>, Error> {
    let db = &data.db;
    let apps = sqlx::query_as::<_, DbApp>("SELECT * FROM apps ORDER BY id ASC")
        .fetch_all(db)
        .await?;
    Ok(apps)
}

pub async fn get_apps_count(data: &Data<AppState>) -> Result<i64, Error> {
    let db = &data.db;
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM apps")
        .fetch_one(db)
        .await?;
    Ok(count)
}

pub async fn create_app(
    data: &Data<AppState>,
    activitypub_id: String,
    url: String,
    name: String,
    description: String,
    is_active: bool,
    image_url: String,
    is_adult: bool,
    tags: String,
) -> Result<(), Error> {
    let db = &data.db;
    sqlx::query("INSERT INTO apps (activitypub_id, url, name, description, is_active, image, is_adult, tags) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)")
        .bind(activitypub_id)
        .bind(url)
        .bind(name)
        .bind(description)
        .bind(is_active)
        .bind(image_url)
        .bind(is_adult)
        .bind(tags)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn update_app(
    data: &Data<AppState>,
    url: String,
    name: String,
    description: String,
    is_active: bool,
    image_url: String,
    is_adult: bool,
    tags: String,
) -> Result<(), Error> {
    let db = &data.db;
    sqlx::query(
        "UPDATE apps SET name = $1, description = $2, is_active = $3, image = $4, is_adult = $5, tags = $6 WHERE url = $7",
    )
    .bind(name)
    .bind(description)
    .bind(is_active)
    .bind(image_url)
    .bind(is_adult)
    .bind(tags)
    .bind(url)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn toggle_app_visibility(id: i32, data: &Data<AppState>) -> Result<(), Error> {
    let db = &data.db;
    sqlx::query("UPDATE apps SET visible = NOT visible WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn get_activity_by_id(id: i32, data: &Data<AppState>) -> Result<DbActivity, Error> {
    let db = &data.db;
    let activity = sqlx::query_as::<_, DbActivity>("SELECT * FROM activities WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(activity)
}

pub async fn get_activities_count(data: &Data<AppState>) -> Result<i64, Error> {
    let db = &data.db;
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM activities")
        .fetch_one(db)
        .await?;
    Ok(count)
}

pub async fn create_activity(
    data: &Data<AppState>,
    activitypub_id: String,
    actor: &str,
    obj: &str,
    kind: &str,
) -> Result<(), Error> {
    let db = &data.db;
    sqlx::query(
        "INSERT INTO activities (activitypub_id, actor, obj, kind) VALUES ($1, $2, $3, $4)",
    )
    .bind(activitypub_id)
    .bind(actor)
    .bind(obj)
    .bind(kind)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn get_relay_by_id(id: i32, data: &Data<AppState>) -> Result<DbRelay, Error> {
    let db = &data.db;
    let relay = sqlx::query_as::<_, DbRelay>("SELECT * FROM relays WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(relay)
}

pub async fn get_relay_by_ap_id(
    ap_id: String,
    data: &Data<AppState>,
) -> Result<Option<DbRelay>, Error> {
    let db = &data.db;
    let relay = sqlx::query_as::<_, DbRelay>("SELECT * FROM relays WHERE activitypub_id = $1")
        .bind(ap_id)
        .fetch_optional(db)
        .await?;
    Ok(relay)
}

pub async fn get_all_relays(data: &Data<AppState>) -> Result<Vec<DbRelay>, Error> {
    let db = &data.db;
    let relays = sqlx::query_as::<_, DbRelay>("SELECT * FROM relays")
        .fetch_all(db)
        .await?;
    Ok(relays)
}

pub async fn create_relay(
    data: &Data<AppState>,
    relay_name: &str,
    activitypub_id: &str,
    inbox: &str,
    outbox: &str,
    public_key: &str,
) -> Result<(), Error> {
    let db = &data.db;
    sqlx::query("INSERT INTO relays (relay_name, activitypub_id, inbox, outbox, public_key, private_key, is_local) VALUES ($1, $2, $3, $4, $5, $6, $7)")
        .bind(relay_name)
        .bind(activitypub_id)
        .bind(inbox)
        .bind(outbox)
        .bind(public_key)
        .bind(None::<String>)
        .bind(false)
        .execute(db)
        .await?;
    Ok(())
}
pub async fn get_relay_follower_id_by_ap_id(
    data: &Data<AppState>,
    ap_id: &str,
) -> Result<i32, Error> {
    let db = &data.db;
    let follower_id: i32 = sqlx::query("SELECT * FROM relays WHERE activitypub_id = $1")
        .bind(ap_id)
        .fetch_one(db)
        .await?
        .try_get("id")?;
    Ok(follower_id)
}

pub async fn get_relay_followers(data: &Data<AppState>) -> Result<Vec<DbRelay>, Error> {
    let db = &data.db;
    let followers = sqlx::query_as(
        "SELECT r.id, r.activitypub_id, r.relay_name, r.inbox, r.outbox, r.public_key, r.private_key, r.is_local \
         FROM followers f \
         JOIN relays r ON f.follower_id = r.id \
         WHERE f.relay_id = 0"
    )
        .fetch_all(db)
        .await?;
    Ok(followers)
}

pub async fn add_follower_to_relay(data: &Data<AppState>, follower_id: i32) -> Result<(), Error> {
    let db = &data.db;
    sqlx::query("INSERT INTO followers (relay_id, follower_id) VALUES ($1, $2)")
        .bind(0) // Only relay system user can be followed
        .bind(follower_id)
        .execute(db)
        .await?;
    Ok(())
}
