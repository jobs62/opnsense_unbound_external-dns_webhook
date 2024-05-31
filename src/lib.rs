pub mod config;
mod external_dns;
mod opnsense;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use config::Config;
use external_dns::{Changes, Edns, Endpoint, Endpoints, Filters, Targets};
use opnsense::unbound;
use opnsense::Opnsense;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::trace::{self, TraceLayer};
use tracing::instrument;

struct AppState {
    config: Config,
    opnsense: Opnsense,
    uuid_map: Mutex<HashMap<String, String>>,
}

pub struct Server {
    config: Config,
}

impl From<Config> for Server {
    fn from(config: Config) -> Self {
        Self { config }
    }
}

impl Server {
    pub async fn serve(&self) -> Result<(), Box<dyn std::error::Error>> {
        let state = Arc::new(AppState {
            opnsense: Opnsense::try_from(&self.config)?,
            config: self.config.clone(),
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

        let listener = tokio::net::TcpListener::bind(&self.config.bind).await?;
        tracing::info!("listening on {}", self.config.bind);

        Ok(axum::serve(listener, app).await?)
    }
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
    let list = state
        .opnsense
        .unbound()
        .settings()
        .search_host_override()
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
    let mut need_restart = false;

    for ep in changes
        .create
        .0
        .iter()
        .map(|c| unbound::Row::from(c.clone()))
    {
        let res = state
            .opnsense
            .unbound()
            .settings()
            .add_host_override(&ep)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let mut guard = state.uuid_map.lock().await;
        guard.insert(ep.domain.clone(), res.uuid);
        need_restart = true;
    }

    for ep in changes
        .update_new
        .0
        .iter()
        .map(|c| unbound::Row::from(c.clone()))
    {
        let guard = state.uuid_map.lock().await;
        if let Some(uuid) = guard.get(&ep.domain) {
            if let Err(e) = state
                .opnsense
                .unbound()
                .settings()
                .set_host_override(uuid, &ep)
                .await
            {
                tracing::error!("update: {:?}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            } else {
                need_restart = true;
            }
        } else {
            tracing::error!("update: could not find uuid in map: {:?}", &ep.domain);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    for ep in changes.delete.0.iter() {
        let mut guard = state.uuid_map.lock().await;
        if let Some(uuid) = guard.get(&ep.dns_name) {
            state
                .opnsense
                .unbound()
                .settings()
                .delete_host_override(uuid)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let _ = guard.remove(&ep.dns_name);
            need_restart = true;
        } else {
            tracing::error!("delete: could not find uuid in map");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    if need_restart {
        let _ = state.opnsense.unbound().service().restart().await;
    }

    Ok(StatusCode::NO_CONTENT)
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
