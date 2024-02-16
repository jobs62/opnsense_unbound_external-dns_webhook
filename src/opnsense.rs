use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::instrument;

#[derive(Deserialize, Debug)]
pub struct OpnsenseSeachResult {
    pub rows: Vec<OpnsenseSeachResultInner>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct OpnsenseSeachResultInner {
    #[serde(skip_serializing)]
    pub uuid: String,
    pub enabled: String,
    pub domain: String,
    pub rr: String,
    pub server: String,
    pub hostname: String,
    pub mx: String,
    pub mxprio: String,
    pub description: String,
}

#[derive(Deserialize, Debug)]
pub struct OpnsenseResponse {
    pub uuid: String,
}

impl From<super::Endpoint> for OpnsenseSeachResultInner {
    fn from(value: super::Endpoint) -> Self {
        OpnsenseSeachResultInner {
            uuid: value.set_identifier.unwrap_or_default(),
            enabled: "1".to_string(),
            domain: value.dns_name,
            rr: value.record_type,
            server: value.targets.0.first().cloned().unwrap_or_default(),
            hostname: "*".to_string(),
            mx: "".to_string(),
            mxprio: "".to_string(),
            description: "".to_string(),
        }
    }
}

#[instrument(skip(state))]
pub async fn list(
    state: &super::AppState,
) -> Result<OpnsenseSeachResult, Box<dyn std::error::Error>> {
    let url = state
        .config
        .base
        .join("api/unbound/settings/searchHostOverride/")?;

    Ok(state
        .client
        .post(url)
        .json(&json!({
            "current": 1,
            "rowCount": -1,
            "searchPhrase": "",
            "sort": {}
        }))
        .basic_auth(&state.config.key, state.config.secret.as_ref())
        .send()
        .await
        .and_then(|r| r.error_for_status())?
        .json::<OpnsenseSeachResult>()
        .await?)
}

#[instrument(skip(state))]
pub async fn delete(state: &super::AppState, uuid: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url = state
        .config
        .base
        .join("api/unbound/settings/delHostOverride/")?
        .join(uuid)?;

    state
        .client
        .post(url)
        .basic_auth(&state.config.key, state.config.secret.as_ref())
        .json(&json!({}))
        .send()
        .await
        .and_then(|r| r.error_for_status())?;

    Ok(())
}

#[instrument(skip(state))]
pub async fn create(
    state: &super::AppState,
    host: &OpnsenseSeachResultInner,
) -> Result<OpnsenseResponse, Box<dyn std::error::Error>> {
    let url = state
        .config
        .base
        .join("api/unbound/settings/addHostOverride/")?;

    Ok(state
        .client
        .post(url)
        .json(&serde_json::json!({
            "host": host
        }))
        .basic_auth(&state.config.key, state.config.secret.as_ref())
        .send()
        .await
        .and_then(|r| r.error_for_status())?
        .json()
        .await?)
}

#[instrument(skip(state))]
pub async fn update(
    state: &super::AppState,
    uuid: &str,
    host: &OpnsenseSeachResultInner,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = state
        .config
        .base
        .join("api/unbound/settings/setHostOverride/")?
        .join(uuid)?;

    state
        .client
        .post(url)
        .json(&serde_json::json!({
            "host": host
        }))
        .basic_auth(&state.config.key, state.config.secret.as_ref())
        .send()
        .await
        .and_then(|r| r.error_for_status())?;

    Ok(())
}
