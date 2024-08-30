use activitypub_federation::{
    config::Data,
    fetch::object_id::ObjectId,
    kinds::activity::{CreateType, FollowType},
    traits::ActivityHandler,
};
use serde::{self, Deserialize, Serialize};
use url::Url;

use crate::error::Error;
use crate::{actors::DbRelay, experiences::DbExperience, EXPERIENCE_LIST};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Follow {
    pub actor: ObjectId<DbRelay>,
    pub object: ObjectId<DbRelay>,
    #[serde(rename = "type")]
    pub kind: FollowType,
    pub id: Url,
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
        let followed = self.object.dereference(data).await?;
        //data.add_follower(followed, actor).await?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Create {
    pub actor: ObjectId<DbRelay>,
    pub object: ObjectId<DbExperience>,
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
        unsafe { EXPERIENCE_LIST.push(experience) }
        Ok(())
    }
}
