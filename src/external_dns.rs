use crate::opnsense::unbound;
use axum::{
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Serialize, Debug)]
pub struct DomainFilter {
    pub filters: Vec<String>,
}
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Endpoints(pub Vec<Endpoint>);

impl IntoIterator for Endpoints {
    type Item = Endpoint;
    type IntoIter = std::vec::IntoIter<Endpoint>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromIterator<Endpoint> for Endpoints {
    fn from_iter<T: IntoIterator<Item = Endpoint>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub dns_name: String,
    #[serde(default)]
    pub targets: Targets,
    pub record_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_ttl: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub provider_specific: Vec<ProviderSpecificProperty>,
}

impl From<unbound::HostOverrideRecord> for Endpoint {
    fn from(value: unbound::HostOverrideRecord) -> Endpoint {
        Endpoint {
            dns_name: format!("{}.{}", value.hostname, value.domain),
            set_identifier: None,
            record_type: value
                .rr
                .split_whitespace()
                .next()
                .map(|s| s.to_string())
                .unwrap_or("A".to_string()),
            targets: Targets(vec![value.server.clone()]),
            ..Default::default()
        }
    }
}

impl Endpoint {
    pub fn get_record_for_zones<'a>(
        &self,
        zones: impl IntoIterator<Item = &'a String>,
    ) -> Option<unbound::HostOverrideRecord> {
        let (host, domain) = self.get_host_and_domain(zones)?;

        Some(unbound::HostOverrideRecord {
            uuid: self.set_identifier.clone().unwrap_or_default(),
            enabled: "1".to_string(),
            domain: domain.clone(),
            rr: self.record_type.clone(),
            server: self.targets[0].clone(),
            hostname: host.clone(),
            mx: "".to_string(),
            mxprio: "".to_string(),
            description: "".to_string(),
        })
    }
    fn get_host_and_domain<'a>(
        &self,
        zones: impl IntoIterator<Item = &'a String>,
    ) -> Option<(String, String)> {
        self.dns_name
            .split_once('.')
            .map(|(host, domain)| (host.into(), domain.into()))
            .filter(|(_, domain)| zones.into_iter().any(|z| z == domain))
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Targets(pub Vec<String>);

impl From<&String> for Targets {
    fn from(value: &String) -> Self {
        Self(vec![value.clone()])
    }
}

impl<I> std::ops::Index<I> for Targets
where
    I: std::slice::SliceIndex<[String]>,
{
    type Output = I::Output;
    fn index(&self, index: I) -> &Self::Output {
        &self.0[index]
    }
}

impl IntoIterator for Targets {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderSpecificProperty {
    name: String,
    value: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct Changes {
    #[serde(deserialize_with = "deserialize_null_default")]
    pub create: Endpoints,
    #[serde(deserialize_with = "deserialize_null_default")]
    _update_old: Endpoints,
    #[serde(deserialize_with = "deserialize_null_default")]
    pub update_new: Endpoints,
    #[serde(deserialize_with = "deserialize_null_default")]
    pub delete: Endpoints,
}

fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

pub struct Edns<T>(pub T);

impl<T> IntoResponse for Edns<T>
where
    T: Serialize,
{
    fn into_response(self) -> axum::response::Response {
        match serde_json::to_string(&self.0) {
            Ok(buf) => (
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/external.dns.webhook+json;version=1"),
                )],
                buf,
            )
                .into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, HeaderValue::from_static("plain/text"))],
                err.to_string(),
            )
                .into_response(),
        }
    }
}
