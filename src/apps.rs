use activitypub_federation::config::Data;
use activitypub_federation::fetch::object_id::ObjectId;
use activitypub_federation::protocol::helpers::deserialize_one_or_many;
use activitypub_federation::protocol::verification::verify_domains_match;
use activitypub_federation::{kinds::object::PageType, traits::Object};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{self, FromRow, Row};
use url::Url;

use crate::error::Error;
use crate::AppState;

/// The internal representation of App data
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DbApp {
    pub ap_id: ObjectId<DbApp>,
    pub url: String,
    pub name: String,
    pub description: String,
    pub active: bool,
}

impl FromRow<'_, sqlx::postgres::PgRow> for DbApp {
    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        let ap_id: &str = row.try_get("activitypub_id")?;
        Ok(Self {
            ap_id: ObjectId::parse(ap_id).unwrap(),
            url: row.try_get("url")?,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            active: row.try_get("is_active")?,
        })
    }
}

impl DbApp {
    pub fn new(
        ap_id: ObjectId<DbApp>,
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
pub struct App {
    #[serde(rename = "type")]
    kind: PageType,
    id: ObjectId<DbApp>,
    pub(crate) attributed_to: String,
    #[serde(deserialize_with = "deserialize_one_or_many")]
    pub(crate) to: Vec<String>,
    content: String,
    name: String,
    summary: String,
}

#[async_trait::async_trait]
impl Object for DbApp {
    type DataType = AppState;
    type Kind = App;
    type Error = Error;

    async fn read_from_id(
        object_id: Url,
        data: &Data<Self::DataType>,
    ) -> Result<Option<Self>, Self::Error> {
        match sqlx::query_as::<_, Self>("SELECT * FROM relay WHERE ap_id = $1")
            .bind(object_id.as_str())
            .fetch_optional(&data.db)
            .await
        {
            Ok(Some(r)) => Ok(Some(r)),
            Ok(None) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn into_json(self, _data: &Data<Self::DataType>) -> Result<Self::Kind, Error> {
        Ok(App {
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

    async fn from_json(
        json: Self::Kind,
        _data: &Data<Self::DataType>,
    ) -> Result<Self, Self::Error> {
        let app = DbApp {
            ap_id: json.id,
            url: json.content,
            name: json.name,
            description: json.summary,
            active: true,
        };
        Ok(app)
    }
}
