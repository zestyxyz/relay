use activitypub_federation::{
    config::Data,
    fetch::object_id::ObjectId,
    kinds::activity::{CreateType, FollowType},
    traits::{ActivityHandler, Actor},
};
use serde::{self, Deserialize, Serialize};
use sqlx::{self, postgres::PgRow, FromRow, Row};
use url::Url;

use crate::error::Error;
use crate::AppState;
use crate::{actors::DbRelay, apps::DbApp};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Follow {
    pub actor: ObjectId<DbRelay>,
    pub object: ObjectId<DbRelay>,
    #[serde(rename = "type")]
    pub kind: FollowType,
    pub id: Url,
}

impl Follow {
    pub fn new(actor: ObjectId<DbRelay>, object: ObjectId<DbRelay>, id: Url) -> Follow {
        Follow {
            actor,
            object,
            kind: Default::default(),
            id,
        }
    }
}

#[async_trait::async_trait]
impl ActivityHandler for Follow {
    type DataType = AppState;
    type Error = Error;

    fn id(&self) -> &Url {
        &self.id
    }

    fn actor(&self) -> &Url {
        self.actor.inner()
    }

    async fn verify(&self, _data: &Data<Self::DataType>) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn receive(self, data: &Data<Self::DataType>) -> Result<(), Self::Error> {
        let actor = self.actor.dereference(data).await?;
        sqlx::query("INSERT INTO relays (name, ap_id, inbox, outbox, public_key, private_key, local) VALUES ($1, $2, $3, $4, $5, $6, $7)")
            .bind(&actor.name)
            .bind(&actor.ap_id.inner().as_str())
            .bind(&actor.inbox.as_str())
            .bind(&actor.outbox.as_str())
            .bind(&actor.public_key_pem())
            .bind(None::<String>)
            .bind(false)
            .execute(&data.db)
            .await?;
        sqlx::query(
            "INSERT INTO activities (actor, object, kind, activity_id) VALUES ($1, $2, $3, $4)",
        )
        .bind(&self.actor.inner().as_str())
        .bind(&self.object.inner().as_str())
        .bind("Follow")
        .bind(&self.id.as_str())
        .execute(&data.db)
        .await?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Create {
    pub actor: ObjectId<DbRelay>,
    pub object: ObjectId<DbApp>,
    #[serde(rename = "type")]
    pub kind: CreateType,
    pub id: Url,
}

#[async_trait::async_trait]
impl ActivityHandler for Create {
    type DataType = AppState;
    type Error = Error;

    fn id(&self) -> &Url {
        &self.id
    }

    fn actor(&self) -> &Url {
        self.actor.inner()
    }

    async fn verify(&self, _data: &Data<Self::DataType>) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn receive(self, data: &Data<Self::DataType>) -> Result<(), Self::Error> {
        let experience = self.object.dereference(data).await?;
        sqlx::query("INSERT INTO apps (activitypub_id, url, name, description, is_active) VALUES ($1, $2, $3, $4, $5)")
            .bind(&experience.ap_id.inner().as_str())
            .bind(&experience.url)
            .bind(&experience.name)
            .bind(&experience.description)
            .bind(&experience.active)
            .execute(&data.db)
            .await?;
        sqlx::query(
            "INSERT INTO activities (actor, object, kind, activity_id) VALUES ($1, $2, $3, $4)",
        )
        .bind(&self.actor.inner().as_str())
        .bind(&self.object.inner().as_str())
        .bind("Create")
        .bind(&self.id.as_str())
        .execute(&data.db)
        .await?;
        Ok(())
    }
}

#[derive(Serialize)]
pub struct DbActivity {
    pub ap_id: ObjectId<DbRelay>,
    pub actor: ObjectId<DbRelay>,
    pub object: ObjectId<DbApp>,
    pub id: Url,
    pub kind: String,
}

impl FromRow<'_, sqlx::postgres::PgRow> for DbActivity {
    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        let actor = row.try_get_raw("actor");
        let actor = actor.unwrap().as_str().unwrap();
        let object = row.try_get_raw("object");
        let object = object.unwrap().as_str().unwrap();
        Ok(Self {
            actor: ObjectId::parse(actor).unwrap(),
            object: ObjectId::parse(object).unwrap(),
            kind: "Follow".to_string(),
            id: Url::parse(row.try_get("id")?).unwrap(),
            ap_id: ObjectId::parse(row.try_get("ap_id")?).unwrap(),
        })
    }
}
