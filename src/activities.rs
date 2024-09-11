use activitypub_federation::{
    config::Data,
    fetch::object_id::ObjectId,
    kinds::activity::{CreateType, FollowType, UpdateType},
    traits::{ActivityHandler, Actor},
};
use serde::{self, Deserialize, Serialize};
use sqlx::{self, postgres::PgRow, FromRow, Row};
use url::Url;

use crate::apps::DbApp;
use crate::db::{
    add_follower_to_relay, create_activity, create_app, create_relay,
    get_relay_follower_id_by_ap_id,
};
use crate::error::Error;
use crate::AppState;
use crate::{actors::DbRelay, db::update_app};

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
        let actor_ap_id = actor.ap_id.inner().as_str();
        create_relay(
            data,
            &actor.name,
            actor_ap_id,
            &actor.inbox.as_str(),
            &actor.outbox.as_str(),
            &actor.public_key_pem(),
        )
        .await?;
        create_activity(
            data,
            self.id.to_string(),
            actor_ap_id,
            &self.object.inner().as_str(),
            "Follow",
        )
        .await?;
        let follower_id = get_relay_follower_id_by_ap_id(data, actor_ap_id).await?;
        add_follower_to_relay(data, follower_id).await?;

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
        let app = self.object.dereference(data).await?;
        create_app(
            data,
            app.ap_id.inner().to_string(),
            app.url,
            app.name,
            app.description,
            app.active,
        )
        .await?;
        create_activity(
            data,
            self.id.to_string(),
            self.actor.inner().as_str(),
            self.object.inner().as_str(),
            "Create",
        )
        .await?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub actor: ObjectId<DbRelay>,
    pub object: ObjectId<DbApp>,
    #[serde(rename = "type")]
    pub kind: UpdateType,
    pub id: Url,
}

#[async_trait::async_trait]
impl ActivityHandler for Update {
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
        let app = self.object.dereference_forced(data).await?;
        update_app(data, app.url, app.name, app.description, app.active).await?;
        create_activity(
            data,
            self.id.to_string(),
            self.actor.inner().as_str(),
            self.object.inner().as_str(),
            "Update",
        )
        .await?;
        Ok(())
    }
}

#[derive(Serialize)]
pub struct DbActivity {
    pub ap_id: ObjectId<DbRelay>,
    pub actor: ObjectId<DbRelay>,
    pub object: ObjectId<DbApp>,
    pub kind: String,
}

impl FromRow<'_, sqlx::postgres::PgRow> for DbActivity {
    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        let actor = row.try_get_raw("actor");
        let actor = actor.unwrap().as_str().unwrap();
        let object = row.try_get_raw("obj");
        let object = object.unwrap().as_str().unwrap();
        Ok(Self {
            ap_id: ObjectId::parse(row.try_get("activitypub_id")?).unwrap(),
            actor: ObjectId::parse(actor).unwrap(),
            object: ObjectId::parse(object).unwrap(),
            kind: row.try_get("kind")?,
        })
    }
}
