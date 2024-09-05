use activitypub_federation::config::Data;

use crate::actors::DbRelay;
use crate::error::Error;
use crate::AppState;

pub async fn get_system_user(data: &Data<AppState>) -> Result<DbRelay, Error> {
    let db = &data.db;
    let user = sqlx::query_as::<_, DbRelay>("SELECT * FROM relays WHERE id = 0 LIMIT 1")
        .fetch_one(db)
        .await?;
    Ok(user)
}
