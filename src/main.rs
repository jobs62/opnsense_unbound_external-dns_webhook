mod opnsense;

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use figment::{
    providers::{Env, Format, Yaml},
    Figment,
};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::trace::{self, TraceLayer};
use tracing::instrument;

#[derive(Serialize, Debug)]
struct Filters {
    filters: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Endpoints(Vec<Endpoint>);

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Endpoint {
    dns_name: String,
    #[serde(default)]
    targets: Targets,
    record_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    set_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    record_ttl: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    provider_specific: Vec<ProviderSpecificProperty>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
struct Targets(Vec<String>);

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ProviderSpecificProperty {
    name: String,
    value: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Changes {
    #[serde(deserialize_with = "deserialize_null_default")]
    create: Endpoints,
    #[serde(deserialize_with = "deserialize_null_default")]
    _update_old: Endpoints,
    #[serde(deserialize_with = "deserialize_null_default")]
    update_new: Endpoints,
    #[serde(deserialize_with = "deserialize_null_default")]
    delete: Endpoints,
}

#[derive(Deserialize, Debug)]
struct Config {
    key: String,
    secret: String,
    #[serde(deserialize_with = "from_str_deserialize")]
    base: reqwest::Url,
    #[serde(default = "default_bind")]
    bind: String,
    #[serde(default)]
    domain_filters: Vec<String>,
    #[serde(default)]
    allow_invalid_certs: bool,
    #[serde(deserialize_with = "deserialize_certificate")]
    certificate_bundle: Vec<reqwest::Certificate>,
}

fn default_bind() -> String {
    "127.0.0.1:8800".to_owned()
}

struct AppState {
    config: Config,
    client: reqwest::Client,
    uuid_map: Mutex<HashMap<String, String>>,
}

fn from_str_deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    let s = String::deserialize(deserializer)?;
    T::from_str(&s).map_err(de::Error::custom)
}

fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

fn deserialize_certificate<'de, D>(deserializer: D) -> Result<Vec<reqwest::Certificate>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match Option::deserialize(deserializer)? {
        None => Vec::new(),
        Some(b) => reqwest::Certificate::from_pem_bundle(b).map_err(de::Error::custom)?,
    })
}

#[tokio::main]
async fn main() {
    let collector = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(collector).unwrap();

    let config: Config = Figment::new()
        .merge(Yaml::file("config.yaml"))
        .merge(Env::prefixed("OPNSENSE_"))
        .extract()
        .unwrap();

    let listener = tokio::net::TcpListener::bind(&config.bind).await.unwrap();
    tracing::info!("listening on {}", &config.bind);

    let mut builder =
        reqwest::Client::builder().danger_accept_invalid_certs(config.allow_invalid_certs);

    for c in config.certificate_bundle.iter() {
        builder = builder.add_root_certificate(c.clone());
    }

    let state = Arc::new(AppState {
        client: builder.build().unwrap(),
        config,
        uuid_map: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/", get(negotiate))
        .route("/healthz", get(healthz))
        .route("/records", get(get_records).post(set_records))
        .route("/adjustendpoints", post(adjust_records))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
                .on_request(trace::DefaultOnRequest::new().level(tracing::Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(tracing::Level::INFO)),
        );

    axum::serve(listener, app).await.unwrap();
}

#[instrument(skip(state))]
async fn negotiate(State(state): State<Arc<AppState>>) -> Result<Edns<Filters>, StatusCode> {
    //TODO: the rest of the implementation doesn't check is domains are valid in regards with those filters

    Ok(Edns(Filters {
        filters: state.config.domain_filters.clone(),
    }))
}

#[instrument(skip(_state))]
async fn healthz(State(_state): State<Arc<AppState>>) -> () {}

#[instrument(skip(state))]
async fn get_records(State(state): State<Arc<AppState>>) -> Result<Edns<Endpoints>, StatusCode> {
    let list = opnsense::list(&state)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut guard = state.uuid_map.lock().await;
    guard.clear();
    for r in list.rows.iter() {
        let _ = guard.insert(r.domain.clone(), r.uuid.clone());
    }
    tracing::debug!("new uuid map: {:?}", guard);
    drop(guard);

    Ok(Edns(Endpoints(
        list.rows
            .into_iter()
            .filter(|r| r.enabled == "1")
            .map(Endpoint::from)
            .collect(),
    )))
}

#[instrument(skip(state))]
async fn set_records(
    State(state): State<Arc<AppState>>,
    Json(changes): Json<Changes>,
) -> Result<StatusCode, StatusCode> {
    for ep in changes
        .create
        .0
        .iter()
        .map(|c| opnsense::OpnsenseSeachResultInner::from(c.clone()))
    {
        let res = opnsense::create(&state, &ep)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let mut guard = state.uuid_map.lock().await;
        guard.insert(ep.domain.clone(), res.uuid);
    }

    for ep in changes
        .update_new
        .0
        .iter()
        .map(|c| opnsense::OpnsenseSeachResultInner::from(c.clone()))
    {
        let guard = state.uuid_map.lock().await;
        if let Some(uuid) = guard.get(&ep.domain) {
            if let Err(e) = opnsense::update(&state, uuid, &ep).await {
                tracing::error!("update: {:?}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        } else {
            tracing::error!("update: could not find uuid in map: {:?}", &ep.domain);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    for ep in changes.delete.0.iter() {
        let mut guard = state.uuid_map.lock().await;
        if let Some(uuid) = guard.get(&ep.dns_name) {
            opnsense::delete(&state, uuid)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let _ = guard.remove(&ep.dns_name);
        } else {
            tracing::error!("delete: could not find uuid in map");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

impl From<opnsense::OpnsenseSeachResultInner> for Endpoint {
    fn from(value: opnsense::OpnsenseSeachResultInner) -> Endpoint {
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

#[instrument(skip(_state))]
async fn adjust_records(
    State(_state): State<Arc<AppState>>,
    Json(endpoints): Json<Endpoints>,
) -> Result<Edns<Endpoints>, StatusCode> {
    let mut results = Endpoints(Vec::new());

    for ep in endpoints.0.iter() {
        match ep.record_type.as_str() {
            "A" | "AAAA" => {}
            _ => continue,
        };

        results.0.push(Endpoint {
            targets: Targets(ep.targets.0[..1].to_vec()),
            record_ttl: None,
            ..ep.clone()
        });
    }

    Ok(Edns(results))
}

struct Edns<T>(pub T);

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
