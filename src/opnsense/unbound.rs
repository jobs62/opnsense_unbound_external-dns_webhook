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
    pub fn diagnostics(&self) -> Diagnostics {
        Diagnostics {
            client: self.client.with_path("diagnostics/").unwrap(),
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

pub struct Diagnostics {
    client: Client,
}

impl Diagnostics {
    pub async fn list_local_zones(&self) -> Result<ListLocalZonesResponse> {
        self.client
            .get::<ListLocalZonesMethod>("listlocalzones/")
            .await
    }
}

struct ListLocalZonesMethod;

impl Method for ListLocalZonesMethod {
    type Response = ListLocalZonesResponse;
}

#[derive(Deserialize, Debug)]
pub struct ListLocalZonesResponse {
    //pub status: String,
    pub data: Vec<Zone>,
}

#[derive(Deserialize, Debug)]
pub struct Zone {
    pub zone: String,
    pub r#type: String,
}

impl Zone {
    // This restricts zone types to those which correspond to
    // the system domain or host overrides. This could be
    // made user configurable in the future with sane defaults.
    pub fn is_allowed_type(&self) -> bool {
        self.r#type.trim().to_lowercase() == "transparent"
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
            _ => Err(anyhow::anyhow!("invalid response format")),
        }
    }
    pub async fn delete_host_override(&self, uuid: &str) -> Result<SettingsUpdateResponse> {
        let res = self
            .client
            .post::<SettingsMethod>(&format!("delHostOverride/{uuid}"), Value::Null)
            .await?;

        match res {
            SettingsResponse::Update(res) => Ok(res),
            _ => Err(anyhow::anyhow!("invalid response format")),
        }
    }
    pub async fn add_host_override(
        &self,
        host: &HostOverrideRecord,
    ) -> Result<SettingsAddResponse> {
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
            _ => Err(anyhow::anyhow!("invalid response format")),
        }
    }
    pub async fn set_host_override(
        &self,
        uuid: &str,
        host: &HostOverrideRecord,
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
            _ => Err(anyhow::anyhow!("invalid response format")),
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
    pub rows: HostOverrideRecords,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct HostOverrideRecords(Vec<HostOverrideRecord>);

impl IntoIterator for HostOverrideRecords {
    type Item = HostOverrideRecord;
    type IntoIter = std::vec::IntoIter<HostOverrideRecord>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct HostOverrideRecord {
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
pub struct SettingsUpdateResponse {
    //pub result: String,
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
    //pub response: Vec<String>,
}
