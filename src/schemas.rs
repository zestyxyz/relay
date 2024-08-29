use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct BeaconPayload {
    pub url: String,
    pub name: String,
    pub description: String,
    pub active: bool,
}

#[derive(Deserialize)]
pub struct RelayPayload {
    pub url: String,
}
