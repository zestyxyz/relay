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
use crate::{actors::DbRelay, apps::DbApp};
use crate::{services::RelayAcceptedActivities, AppState};

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
        unsafe {
            let new_relay = DbRelay::new(
                actor.name.clone(),
                actor.ap_id.clone(),
                actor.inbox.clone(),
                actor.outbox.clone(),
                actor.public_key_pem().to_string(),
                None,
                false,
            );
            RELAYS.push(new_relay)
        }
        unsafe {
            ACTIVITIES.push(RelayAcceptedActivities::Follow(self));
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, FromRow)]
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
        unsafe {
            APPS_LIST.push(experience);
            ACTIVITIES.push(RelayAcceptedActivities::Create(self));
        }
        Ok(())
    }
}

pub struct DbActivity {
    pub ap_id: ObjectId<DbRelay>,
    pub actor: ObjectId<DbRelay>,
    pub object: ObjectId<DbApp>,
    pub id: Url,
    pub kind: String,
}
