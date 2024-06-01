use crate::opnsense::unbound;
use axum::{
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Serialize, Debug)]
pub struct Filters {
    pub filters: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Endpoints(pub Vec<Endpoint>);

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

impl From<unbound::Row> for Endpoint {
    fn from(value: unbound::Row) -> Endpoint {
        Endpoint {
            dns_name: value.domain.clone(),
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

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Targets(pub Vec<String>);

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
