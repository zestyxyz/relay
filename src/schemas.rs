use serde::Deserialize;

#[derive(Deserialize)]
pub struct BeaconPayload {
    pub url: String,
    pub name: String,
    pub description: String,
    pub active: bool,
}
