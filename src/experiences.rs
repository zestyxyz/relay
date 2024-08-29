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

impl Object for Experience {
    type DataType = ();
    type Kind = Page;
    type Error = Error;

    fn read_from_id<'life0, 'async_trait>(
        object_id: Url,
        data: &'life0 Data<Self::DataType>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Self>, Self::Error>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn into_json<'life0, 'async_trait>(
        self,
        data: &'life0 Data<Self::DataType>,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Kind, Self::Error>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn verify<'life0, 'life1, 'life2, 'async_trait>(
        json: &'life0 Self::Kind,
        expected_domain: &'life1 Url,
        data: &'life2 Data<Self::DataType>,
    ) -> ::core::pin::Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn from_json<'life0, 'async_trait>(
        json: Self::Kind,
        data: &'life0 Data<Self::DataType>,
    ) -> Pin<Box<dyn Future<Output = Result<Self, Self::Error>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }
}
