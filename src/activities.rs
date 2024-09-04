use activitypub_federation::{
    config::Data,
    fetch::object_id::ObjectId,
    kinds::activity::{CreateType, FollowType},
    traits::{ActivityHandler, Actor},
};
use serde::{self, Deserialize, Serialize};
use url::Url;

use crate::{error::Error, ACTIVITIES, RELAYS};
use crate::{actors::DbRelay, apps::DbApp, APPS_LIST};
use crate::RelayAcceptedActivities;

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
    type DataType = ();
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
        unsafe { ACTIVITIES.push(RelayAcceptedActivities::Follow(self)); }
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
    type DataType = ();
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
