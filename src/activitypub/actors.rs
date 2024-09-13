use std::fmt::Debug;
use std::str::FromStr;

use activitypub_federation::activity_queue::queue_activity;
use activitypub_federation::activity_sending::SendActivityTask;
use activitypub_federation::config::Data;
use activitypub_federation::fetch::webfinger::webfinger_resolve_actor;
use activitypub_federation::protocol::context::WithContext;
use activitypub_federation::protocol::verification::verify_domains_match;
use activitypub_federation::traits::{ActivityHandler, Actor};
use activitypub_federation::{
    fetch::object_id::ObjectId, kinds::actor::ServiceType, protocol::public_key::PublicKey,
    traits::Object,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{self, FromRow, Row};
use url::Url;

use super::activities::Follow;
use super::db::{create_activity, get_activities_count, get_relay_by_ap_id};
use super::error::Error;
use crate::AppState;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Relay {
    pub id: ObjectId<DbRelay>,
    #[serde(rename = "type")]
    pub kind: ServiceType,
    pub preferred_username: String,
    pub name: String,
    pub inbox: Url,
    pub outbox: Url,
    pub public_key: PublicKey,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DbRelay {
    pub name: String,
    pub ap_id: ObjectId<DbRelay>,
    pub inbox: Url,
    pub outbox: Url,
    // exists for all users (necessary to verify http signatures)
    public_key: String,
    // exists only for local users
    private_key: Option<String>,
    pub last_refreshed_at: DateTime<Utc>,
    pub local: bool,
}

impl DbRelay {
    pub fn new(
        name: String,
        ap_id: ObjectId<DbRelay>,
        inbox: Url,
        outbox: Url,
        public_key: String,
        private_key: Option<String>,
        local: bool,
    ) -> Self {
        Self {
            name,
            ap_id,
            inbox,
            outbox,
            public_key,
            private_key,
            last_refreshed_at: Utc::now(),
            local,
        }
    }

    pub(crate) async fn send<Activity>(
        &self,
        activity: Activity,
        recipients: Vec<Url>,
        use_queue: bool,
        data: &Data<AppState>,
    ) -> Result<(), Error>
    where
        Activity: ActivityHandler + Serialize + Debug + Send + Sync,
        <Activity as ActivityHandler>::Error: From<Error> + From<serde_json::Error>,
    {
        let activity = WithContext::new_default(activity);
        // Send through queue in some cases and bypass it in others to test both code paths
        if use_queue {
            queue_activity(&activity, self, recipients, data).await?;
        } else {
            let sends = SendActivityTask::prepare(&activity, self, recipients, data).await?;
            for send in sends {
                send.sign_and_send(data).await?;
            }
        }
        Ok(())
    }

    pub fn followers_url(&self) -> Result<Url, Error> {
        Ok(Url::parse(&format!("{}/followers", self.ap_id.inner()))?)
    }

    pub async fn follow(&self, other: &str, data: &Data<AppState>) -> Result<(), Error> {
        let other: DbRelay = webfinger_resolve_actor(other, data).await?;
        let activities_count = match get_activities_count(data).await {
            Ok(count) => count,
            Err(e) => panic!("Error fetching apps count: {}", e),
        };
        let follow = Follow::new(
            self.ap_id.clone(),
            other.ap_id.clone(),
            Url::from_str(&format!(
                "{}/activities/{}",
                self.ap_id.inner().as_str(),
                activities_count
            ))?,
        );
        match create_activity(
            data,
            format!(
                "{}/activities/{}",
                self.ap_id.inner().as_str(),
                activities_count
            ),
            self.ap_id.inner().as_str(),
            other.ap_id.inner().as_str(),
            "Follow",
        )
        .await
        {
            Ok(_) => {
                self.send(follow, vec![other.shared_inbox_or_inbox()], false, data)
                    .await?;
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }
}

impl FromRow<'_, sqlx::postgres::PgRow> for DbRelay {
    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        let ap_id = row.try_get_raw("activitypub_id");
        let ap_id = ap_id.unwrap().as_str().unwrap();
        Ok(Self {
            ap_id: ObjectId::parse(ap_id).unwrap(),
            name: row.try_get("relay_name")?,
            inbox: Url::from_str(row.try_get("inbox")?).unwrap(),
            outbox: Url::from_str(row.try_get("outbox")?).unwrap(),
            public_key: row.try_get("public_key")?,
            private_key: row.try_get("private_key")?,
            last_refreshed_at: Utc::now(),
            local: row.try_get("is_local")?,
        })
    }
}

#[async_trait::async_trait]
impl Object for DbRelay {
    type DataType = AppState;

    type Kind = Relay;

    type Error = Error;

    async fn read_from_id(
        object_id: Url,
        data: &Data<Self::DataType>,
    ) -> Result<Option<Self>, Self::Error> {
        match get_relay_by_ap_id(object_id.to_string(), data).await {
            Ok(Some(r)) => Ok(Some(r)),
            Ok(None) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn into_json(self, _data: &Data<Self::DataType>) -> Result<Self::Kind, Self::Error> {
        let name = self.name.clone();
        let owner = self.ap_id.inner().clone();
        let public_key_pem = self.public_key.clone();
        Ok(Relay {
            id: self.ap_id,
            kind: ServiceType::Service,
            preferred_username: "".to_string(),
            name: name.clone(),
            inbox: self.inbox,
            outbox: self.outbox,
            public_key: PublicKey {
                id: name,
                owner,
                public_key_pem,
            },
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
        let user = DbRelay {
            name: json.preferred_username,
            ap_id: json.id,
            inbox: json.inbox,
            outbox: json.outbox,
            public_key: json.public_key.public_key_pem,
            private_key: None,
            last_refreshed_at: Utc::now(),
            local: false,
        };
        Ok(user)
    }
}

#[async_trait::async_trait]
impl Actor for DbRelay {
    fn id(&self) -> Url {
        self.ap_id.inner().clone()
    }

    fn public_key_pem(&self) -> &str {
        &self.public_key
    }

    fn private_key_pem(&self) -> Option<String> {
        self.private_key.clone()
    }

    fn inbox(&self) -> Url {
        self.inbox.clone()
    }
}
