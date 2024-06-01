use crate::external_dns::Endpoint;
use crate::opnsense::client::Method;
use crate::opnsense::{Client, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
pub struct Unbound {
    client: Client,
}

impl Unbound {
    pub fn new(client: Client) -> Self {
        Self {
            client: client.with_path("unbound/").unwrap(),
        }
    }
    pub fn settings(&self) -> Settings {
        Settings {
            client: self.client.with_path("settings/").unwrap(),
        }
    }
    pub fn service(&self) -> Service {
        Service {
            client: self.client.with_path("service/").unwrap(),
        }
    }
}

pub struct Settings {
    client: Client,
}

impl Settings {
    pub async fn search_host_override(&self) -> Result<SettingsListResponse> {
        let res = self
            .client
            .post::<SettingsMethod>(
                "searchHostOverride/",
                json!({
                    "current": 1,
                    "rowCount": -1,
                    "searchPhrase": "",
                    "sort": {}
                }),
            )
            .await?;

        match res {
            SettingsResponse::List(res) => Ok(res),
            _ => Err("invalid response format".into()),
        }
    }
    pub async fn delete_host_override(&self, uuid: &str) -> Result<SettingsUpdateResponse> {
        let res = self
            .client
            .post::<SettingsMethod>(&format!("delHostOverride/{uuid}"), Value::Null)
            .await?;

        match res {
            SettingsResponse::Update(res) => Ok(res),
            _ => Err("invalid response format".into()),
        }
    }
    pub async fn add_host_override(&self, host: &Row) -> Result<SettingsAddResponse> {
        let res = self
            .client
            .post::<SettingsMethod>(
                "addHostOverride/",
                json!({
                    "host": host
                }),
            )
            .await?;

        match res {
            SettingsResponse::Add(res) => Ok(res),
            _ => Err("invalid response format".into()),
        }
    }
    pub async fn set_host_override(
        &self,
        uuid: &str,
        host: &Row,
    ) -> Result<SettingsUpdateResponse> {
        let res = self
            .client
            .post::<SettingsMethod>(
                &format!("setHostOverride/{uuid}"),
                json!({
                    "host": host
                }),
            )
            .await?;

        match res {
            SettingsResponse::Update(res) => Ok(res),
            _ => Err("invalid response format".into()),
        }
    }
}

struct SettingsMethod;

impl Method for SettingsMethod {
    type Response = SettingsResponse;
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum SettingsResponse {
    Add(SettingsAddResponse),
    List(SettingsListResponse),
    Update(SettingsUpdateResponse),
}

#[derive(Deserialize, Debug)]
pub struct SettingsAddResponse {
    pub uuid: String,
}

#[derive(Deserialize, Debug)]
pub struct SettingsListResponse {
    pub rows: Vec<Row>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Row {
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

impl From<Endpoint> for Row {
    fn from(value: Endpoint) -> Self {
        Row {
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

#[derive(Deserialize, Debug)]
pub struct SettingsUpdateResponse {
    pub result: String,
}

pub struct Service {
    client: Client,
}

impl Service {
    pub async fn restart(&self) -> Result<ServiceRestartResponse> {
        let res = self
            .client
            .post::<ServiceMethod>("restart/", Value::Null)
            .await?;

        match res {
            ServiceResponse::Restart(res) => Ok(res),
        }
    }
}

struct ServiceMethod;

impl Method for ServiceMethod {
    type Response = ServiceResponse;
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum ServiceResponse {
    Restart(ServiceRestartResponse),
}

#[derive(Deserialize, Debug)]
pub struct ServiceRestartResponse {
    pub response: Vec<String>,
}
