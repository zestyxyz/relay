use activitypub_federation::config::Data;
use activitypub_federation::error::Error;
use activitypub_federation::fetch::object_id::ObjectId;
use activitypub_federation::protocol::verification::verify_domains_match;
use activitypub_federation::protocol::helpers::deserialize_one_or_many;
use activitypub_federation::{kinds::object::PageType, traits::Object};
use serde::{Deserialize, Serialize};
use url::Url;

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
    pub(crate) attributed_to: String,
    #[serde(deserialize_with = "deserialize_one_or_many")]
    pub(crate) to: Vec<String>,
    content: String,
    name: String,
    summary: String,
}

#[async_trait::async_trait]
impl Object for DbExperience {
    type DataType = ();
    type Kind = Experience;
    type Error = Error;

    async fn read_from_id(
        object_id: Url,
        _data: &Data<Self::DataType>,
    ) -> Result<Option<Self>, Self::Error> {
        let experience = unsafe {
            EXPERIENCE_LIST.iter().find(|e| *e.ap_id.inner() == object_id).cloned()
        };
        Ok(experience)
    }

    async fn into_json(self, _data: &Data<Self::DataType>) -> Result<Self::Kind, Error> {
        Ok(Experience {
            id: self.ap_id,
            kind: PageType::Page,
            attributed_to: "".to_string(),
            to: vec![],
            name: self.name,
            summary: self.description,
            content: self.url,
            // TODO: Add in the other fields
        })
    }

    async fn verify(
        json: &Self::Kind,
        expected_domain: &Url,
        _data: &Data<Self::DataType>,
    ) -> Result<(), Self::Error> {
        verify_domains_match(json.id.inner(), expected_domain)?;
        Ok(())
    }

    async fn from_json(json: Self::Kind, _data: &Data<Self::DataType>) -> Result<Self, Self::Error> {
        let experience = DbExperience {
            ap_id: json.id,
            url: json.content,
            name: json.name,
            description: json.summary,
            active: false,
        };
        Ok(experience)
    }
}
