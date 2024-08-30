use activitypub_federation::config::Data;
use activitypub_federation::error::Error;
use activitypub_federation::protocol::helpers::deserialize_one_or_many;
use activitypub_federation::{kinds::object::PageType, traits::Object};
use serde::{Deserialize, Serialize};
use url::Url;

use core::future::Future;
use core::marker::Send;
use core::pin::Pin;

/// The internal representation of experience data
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Experience {
    pub url: String,
    pub name: String,
    pub description: String,
    pub active: bool,
}

impl Experience {
    pub fn new(url: String, name: String, description: String, active: bool) -> Self {
        Self {
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
pub struct Page {
    #[serde(rename = "type")]
    kind: PageType,
    id: String,                       //placeholder
    pub(crate) attributed_to: String, //placeholder
    #[serde(deserialize_with = "deserialize_one_or_many")]
    pub(crate) to: Vec<String>, //placeholder
    content: String,                  // experiencec URL
}

#[async_trait::async_trait]
impl Object for Experience {
    type DataType = ();
    type Kind = Page;
    type Error = Error;

    async fn read_from_id(
        object_id: Url,
        data: &Data<Self::DataType>,
    ) -> Result<Option<Self>, Self::Error> {
        todo!()
    }

    async fn into_json(self, data: &Data<Self::DataType>) -> Result<Self::Kind, Self::Error> {
        todo!()
    }

    async fn verify(
        json: &Self::Kind,
        expected_domain: &Url,
        data: &Data<Self::DataType>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn from_json(json: Self::Kind, data: &Data<Self::DataType>) -> Result<Self, Self::Error> {
        todo!()
    }
}
