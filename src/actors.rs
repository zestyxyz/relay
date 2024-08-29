use activitypub_federation::config::Data;
use activitypub_federation::traits::Actor;
use activitypub_federation::{
    error::Error, fetch::object_id::ObjectId, kinds::actor::ServiceType,
    protocol::public_key::PublicKey, traits::Object,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

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

#[derive(Debug)]
pub struct DbRelay {
    pub name: String,
    pub ap_id: ObjectId<DbRelay>,
    pub inbox: Url,
    // exists for all users (necessary to verify http signatures)
    public_key: String,
    // exists only for local users
    private_key: Option<String>,
    last_refreshed_at: DateTime<Utc>,
    pub followers: Vec<Url>,
    pub local: bool,
}

#[async_trait::async_trait]
impl Object for DbRelay {
    type DataType = ();

    type Kind = Relay;

    type Error = Error;

    async fn read_from_id(
        object_id: Url,
        data: &Data<Self::DataType>,
    ) -> Result<Option<Self>, Self::Error> {
        // let users = data.users.lock().unwrap();
        // let res = users
        //     .clone()
        //     .into_iter()
        //     .find(|u| u.ap_id.inner() == &object_id);
        // Ok(res)
        Ok(None)
    }

    async fn into_json(self, _data: &Data<Self::DataType>) -> Result<Self::Kind, Self::Error> {
        Ok(Relay {
            id: todo!(),
            kind: ServiceType::Service,
            preferred_username: todo!(),
            name: todo!(),
            inbox: todo!(),
            outbox: todo!(),
            public_key: todo!(),
        })
    }

    async fn verify(
        json: &Self::Kind,
        expected_domain: &Url,
        _data: &Data<Self::DataType>,
    ) -> Result<(), Self::Error> {
        // verify_domains_match(json.id.inner(), expected_domain)?;
        Ok(())
    }

    async fn from_json(json: Self::Kind, data: &Data<Self::DataType>) -> Result<Self, Self::Error> {
        let user = DbRelay {
            name: json.preferred_username,
            ap_id: json.id,
            inbox: json.inbox,
            public_key: json.public_key.public_key_pem,
            private_key: None,
            last_refreshed_at: Utc::now(),
            followers: vec![],
            local: false,
        };
        // let mut mutex = data.users.lock().unwrap();
        // mutex.push(user.clone());
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
