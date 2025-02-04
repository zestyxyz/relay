use std::env;

use activitypub_federation::config::Data;
use activitypub_federation::fetch::object_id::ObjectId;
use activitypub_federation::protocol::helpers::{deserialize_one_or_many, deserialize_skip_error};
use activitypub_federation::protocol::verification::verify_domains_match;
use activitypub_federation::{kinds::object::PageType, traits::Object};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{self, FromRow, Row};
use url::Url;

use super::db::get_app_by_ap_id;
use super::error::Error;
use crate::AppState;

/// The internal representation of App data
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DbApp {
    pub id: i32,
    pub ap_id: ObjectId<DbApp>,
    pub url: String,
    pub name: String,
    pub description: String,
    pub active: bool,
    pub image: String,
    pub adult: bool,
    pub tags: String,
    pub visible: bool,
}

impl FromRow<'_, sqlx::postgres::PgRow> for DbApp {
    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        let ap_id: &str = row.try_get("activitypub_id")?;
        Ok(Self {
            id: row.try_get("id")?,
            ap_id: ObjectId::parse(ap_id).unwrap(),
            url: row.try_get("url")?,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            active: row.try_get("is_active")?,
            image: row.try_get("image")?,
            adult: row.try_get("is_adult")?,
            tags: row.try_get("tags")?,
            visible: row.try_get("visible")?,
        })
    }
}

impl DbApp {
    pub fn new(
        id: i32,
        ap_id: ObjectId<DbApp>,
        url: String,
        name: String,
        description: String,
        active: bool,
        image: String,
        adult: bool,
        tags: String,
        visible: bool,
    ) -> Self {
        Self {
            id,
            ap_id,
            url,
            name,
            description,
            active,
            image,
            adult,
            tags,
            visible,
        }
    }

    pub fn page_url(&self) -> String {
        let domain = env::var("DOMAIN").expect("DOMAIN must be set");
        let protocol = env::var("PROTOCOL").expect("PROTOCOL must be set");
        let full_domain = format!("{}{}", protocol, domain);

        let ap_id = self.ap_id.clone().into_inner();
        let idx = ap_id
            .as_str()
            .split("/")
            .last()
            .expect("This app should have an index!");
        format!("{}/app/{}", full_domain, idx)
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct APImage {
    #[serde(rename = "type")]
    kind: String,
    href: String,
    media_type: String,
}

impl APImage {
    pub fn new(href: String) -> Self {
        Self {
            kind: "Image".to_string(),
            href,
            media_type: "image/png".to_string(),
        }
    }
}

/// How the experiencce is serialized and represented as Activitypub JSON
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct App {
    #[serde(rename = "type")]
    kind: PageType,
    app_id: i32,
    id: ObjectId<DbApp>,
    pub(crate) attributed_to: String,
    #[serde(deserialize_with = "deserialize_one_or_many")]
    pub(crate) to: Vec<String>,
    content: String,
    name: String,
    summary: String,
    #[serde(deserialize_with = "deserialize_skip_error", default)]
    image: Option<APImage>,
    sensitive: bool,
    // Non-standard field
    tags: String,
}

impl App {
    pub fn new(
        app_id: i32,
        id: ObjectId<DbApp>,
        attributed_to: String,
        to: Vec<String>,
        content: String,
        name: String,
        summary: String,
        image: Option<APImage>,
        sensitive: bool,
        tags: String,
    ) -> Self {
        Self {
            app_id,
            kind: PageType::Page,
            id,
            attributed_to,
            to,
            content,
            name,
            summary,
            image,
            sensitive,
            tags,
        }
    }
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
        match get_app_by_ap_id(data, object_id.as_str()).await {
            Ok(Some(r)) => Ok(Some(r)),
            Ok(None) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn into_json(self, _data: &Data<Self::DataType>) -> Result<Self::Kind, Error> {
        Ok(App {
            app_id: self.id,
            id: self.ap_id,
            kind: PageType::Page,
            attributed_to: "".to_string(),
            to: vec![],
            name: self.name,
            summary: self.description,
            content: self.url,
            image: Some(APImage::new(self.image)),
            sensitive: self.adult,
            tags: self.tags,
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
        let image = json.image.and_then(|i| Some(i.href));
        let app = DbApp {
            id: json.app_id,
            ap_id: json.id,
            url: json.content,
            name: json.name,
            description: json.summary,
            active: true,
            image: image.unwrap_or("".to_string()),
            adult: json.sensitive,
            tags: json.tags,
            visible: true,
        };
        Ok(app)
    }
}
