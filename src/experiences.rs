use activitypub_federation::config::Data;
use activitypub_federation::error::Error;
use activitypub_federation::fetch::object_id::ObjectId;
use activitypub_federation::kinds::object;
use activitypub_federation::protocol::helpers::deserialize_one_or_many;
use activitypub_federation::{kinds::object::PageType, traits::Object};
use serde::{Deserialize, Serialize};
use url::Url;

use core::future::Future;
use core::marker::Send;
use core::pin::Pin;
use std::str::FromStr;

use crate::EXPERIENCE_LIST;

/// The internal representation of experience data
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DbExperience {
    pub ap_id: ObjectId<DbExperience>,
    pub url: String,
    pub name: String,
    pub description: String,
    pub active: bool,
}

impl DbExperience {
    pub fn new(
        ap_id: ObjectId<DbExperience>,
        url: String,
        name: String,
        description: String,
        active: bool,
    ) -> Self {
        Self {
            ap_id,
            url,
            name,
            description,
            active,
        }
    }
}

/// How the experiencce is serialized and represented as Activitypub JSON
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Experience {
    #[serde(rename = "type")]
    kind: PageType,
    id: ObjectId<DbExperience>,
    pub(crate) attributed_to: String, //placeholder
    #[serde(deserialize_with = "deserialize_one_or_many")]
    pub(crate) to: Vec<String>, //placeholder
    content: String,                  // experiencec URL
}

#[async_trait::async_trait]
impl Object for DbExperience {
    type DataType = ();
    type Kind = Experience;
    type Error = Error;

    async fn read_from_id(
        object_id: Url,
        data: &Data<Self::DataType>,
    ) -> Result<Option<Self>, Self::Error> {
        let experience = DbExperience {
            ap_id: ObjectId::<DbExperience>::from_str(object_id.as_str()).unwrap(),
            url: object_id.to_string(),
            name: "Test Experience".to_string(),
            description: "Test Description".to_string(),
            active: true,
        };
        Ok(Some(experience))
    }

    async fn into_json(self, data: &Data<Self::DataType>) -> Result<Self::Kind, Self::Error> {
        Ok(Experience {
            id: todo!(),
            kind: PageType::Page,
            attributed_to: todo!(),
            to: todo!(),
            content: todo!(),
        })
    }

    async fn verify(
        json: &Self::Kind,
        expected_domain: &Url,
        data: &Data<Self::DataType>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn from_json(json: Self::Kind, data: &Data<Self::DataType>) -> Result<Self, Self::Error> {
        let experience = DbExperience {
            ap_id: json.id,
            url: json.content,
            name: "".to_string(),
            description: "".to_string(),
            active: false,
        };
        Ok(experience)
    }
}
