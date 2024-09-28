pub mod config;
mod external_dns;
mod opnsense;
mod state;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use config::Config;
use external_dns::{Changes, DomainFilter, Edns, Endpoint, Endpoints};
use opnsense::Opnsense;
use state::{AppState, DefaultRecordCache, DefaultZoneCache, RecordCache, ZoneCache};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::trace::{self, TraceLayer};
use tracing::instrument;

pub struct Server {
    config: Config,
}

impl From<Config> for Server {
    fn from(config: Config) -> Self {
        Self { config }
    }
}

impl Server {
    pub async fn serve(&self) -> anyhow::Result<()> {
        let state = AppState {
            opnsense: Opnsense::try_from(&self.config)?,
            config: self.config.clone(),
            record_cache: Arc::new(RwLock::new(DefaultRecordCache::new())),
            zone_cache: Arc::new(RwLock::new(DefaultZoneCache::new())),
        };

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

// negotiate retrieves filtered zones from Opnsense
// and responds to external-dns with zone filters.
#[instrument(skip(state))]
async fn negotiate<R: RecordCache, Z: ZoneCache>(
    State(state): State<AppState<R, Z>>,
) -> Result<Edns<DomainFilter>, StatusCode> {
    let zones = zones(&state)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!(?zones, "replying with filtered zones");

    Ok(Edns(DomainFilter { filters: zones }))
}

#[instrument(skip(_state))]
async fn healthz<R: RecordCache, Z: ZoneCache>(State(_state): State<AppState<R, Z>>) -> () {}

// Gets existing host overrides and filters by managed zones
// Updates UUID map to map records to their UUID's
// Returns "enabled" records as endpoints
#[instrument(skip(state))]
async fn get_records<R: RecordCache, Z: ZoneCache>(
    State(state): State<AppState<R, Z>>,
) -> Result<Edns<Endpoints>, StatusCode> {
    let list = state
        .opnsense
        .unbound()
        .settings()
        .search_host_override()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let zones = zones(&state)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let records = list.rows.into_iter().filter(|r| zones.contains(&r.domain));

    let mut guard = state.record_cache.write().await;

    guard.clear();

    for r in records.clone() {
        guard
            .try_insert_record(&r)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    drop(guard);

    Ok(Edns(Endpoints::from_iter(
        records
            .filter(|r| r.enabled == "1")
            .map(Into::into)
            .inspect(|f: &Endpoint| tracing::info!("records: {:?}", f)),
    )))
}

#[instrument(skip(state, changes))]
async fn set_records<R: RecordCache, Z: ZoneCache>(
    State(state): State<AppState<R, Z>>,
    Json(changes): Json<Changes>,
) -> Result<StatusCode, StatusCode> {
    let (creates, updates, deletes) = process_changes(&state, changes)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let results = [
        create_records(&state, creates).await,
        update_records(&state, updates).await,
        delete_records(&state, deletes).await,
    ]
    .into_iter()
    .collect::<Result<Vec<_>, _>>();

    match results {
        Ok(res) => {
            for out in &res {
                tracing::info!("{}", out);
            }

            if res.iter().any(|o| o.requires_restart()) {
                let _ = state.opnsense.unbound().service().restart().await;
            }

            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            tracing::error!("{e}");

            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn process_changes<R: RecordCache, Z: ZoneCache>(
    state: &AppState<R, Z>,
    changes: Changes,
) -> anyhow::Result<(
    Vec<opnsense::unbound::HostOverrideRecord>,
    Vec<opnsense::unbound::HostOverrideRecord>,
    Vec<opnsense::unbound::HostOverrideRecord>,
)> {
    let zones = zones(state).await?;

    let mut creates: Vec<opnsense::unbound::HostOverrideRecord> = vec![];
    let mut updates: Vec<opnsense::unbound::HostOverrideRecord> = changes
        .update_new
        .into_iter()
        .flat_map(|ep| ep.get_record_for_zones(&zones))
        .collect();
    let deletes: Vec<opnsense::unbound::HostOverrideRecord> = changes
        .delete
        .into_iter()
        .flat_map(|ep| ep.get_record_for_zones(&zones))
        .collect();

    let guard = state.record_cache.read().await;

    for record in changes
        .create
        .into_iter()
        .flat_map(|ep| ep.get_record_for_zones(&zones))
    {
        match guard.try_get_record(&record)? {
            Some(e) if e.enabled => continue,
            Some(_) => updates.push(record),
            None => creates.push(record),
        }
    }

    Ok((creates, updates, deletes))
}

#[instrument(skip(state, creates))]
async fn create_records<R: RecordCache, Z: ZoneCache>(
    state: &AppState<R, Z>,
    creates: impl IntoIterator<Item = opnsense::unbound::HostOverrideRecord>,
) -> anyhow::Result<Output> {
    let mut output = Output::new(Operation::Create);

    for mut record in creates {
        output.records_requested += 1;

        let res = state
            .opnsense
            .unbound()
            .settings()
            .add_host_override(&record)
            .await?;

        tracing::debug!(?record, "added host override");

        output.records_processed += 1;

        record.uuid = res.uuid;

        state
            .record_cache
            .write()
            .await
            .try_insert_record(&record)?;
    }

    Ok(output)
}

#[instrument(skip(state, updates))]
async fn update_records<R: RecordCache, Z: ZoneCache>(
    state: &AppState<R, Z>,
    updates: impl IntoIterator<Item = opnsense::unbound::HostOverrideRecord>,
) -> anyhow::Result<Output> {
    let mut output = Output::new(Operation::Update);

    let guard = state.record_cache.read().await;

    for record in updates {
        output.records_requested += 1;

        let entry = guard.try_get_record(&record)?.ok_or(anyhow::anyhow!(
            "could not find uuid in map: {}.{}",
            &record.hostname,
            &record.domain
        ))?;

        tracing::debug!(?entry, "updating host override");

        output.records_processed += 1;

        state
            .opnsense
            .unbound()
            .settings()
            .set_host_override(&entry.uuid, &record)
            .await?;
    }

    Ok(output)
}

#[instrument(skip(state, deletes))]
async fn delete_records<R: RecordCache, Z: ZoneCache>(
    state: &AppState<R, Z>,
    deletes: impl IntoIterator<Item = opnsense::unbound::HostOverrideRecord>,
) -> anyhow::Result<Output> {
    let mut output = Output::new(Operation::Delete);

    let mut guard = state.record_cache.write().await;

    for record in deletes.into_iter() {
        output.records_requested += 1;

        let entry = guard
            .try_get_record(&record)?
            .ok_or(anyhow::anyhow!("could not find uuid in map: {:?}", &record))?;

        tracing::debug!(?entry, "deleting host override");

        output.records_processed += 1;

        state
            .opnsense
            .unbound()
            .settings()
            .delete_host_override(&entry.uuid)
            .await?;

        guard.try_remove_record(&record)?;
    }

    Ok(output)
}

struct Output {
    operation: Operation,
    pub records_requested: u64,
    pub records_processed: u64,
}

impl Output {
    pub fn new(op: Operation) -> Self {
        Self {
            operation: op,
            records_requested: 0,
            records_processed: 0,
        }
    }
    pub fn requires_restart(&self) -> bool {
        self.records_processed > 0
    }
}

impl std::fmt::Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: records_requested: {}, records_processed: {}",
            self.operation, self.records_requested, self.records_processed
        )
    }
}

enum Operation {
    Create,
    Update,
    Delete,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        };

        write!(f, "{s}")
    }
}

#[instrument(skip(_state))]
async fn adjust_records<R: RecordCache, Z: ZoneCache>(
    State(_state): State<AppState<R, Z>>,
    Json(endpoints): Json<Endpoints>,
) -> Result<Edns<Endpoints>, StatusCode> {
    let records: Endpoints = endpoints
        .0
        .iter()
        .filter(|ep| ["A", "AAAA"].contains(&ep.record_type.as_str()))
        .map(|ep| Endpoint {
            record_ttl: None,
            targets: (&ep.targets[0]).into(),
            ..ep.clone()
        })
        .collect();

    tracing::info!(
        "records_requested: {}, records_adjusted: {}",
        endpoints.0.len(),
        records.0.len()
    );

    Ok(Edns(records))
}

// zones retrieves zones from Opnsense, filters,
// and caches them. Zones are returned from cache
// on subsequent calls.
#[instrument(skip(state))]
async fn zones<R: RecordCache, Z: ZoneCache>(
    state: &AppState<R, Z>,
) -> anyhow::Result<Vec<String>> {
    let filters = &state.config.domain_filters;
    let mut guard = state.zone_cache.write().await;
    let zones = guard.values();
    if !zones.is_empty() {
        return Ok(zones);
    }

    let zones = state
        .opnsense
        .unbound()
        .diagnostics()
        .list_local_zones()
        .await?
        .data
        .iter()
        .filter_map(|z| z.is_allowed_type().then_some(&z.zone))
        .flat_map(|z| z.strip_suffix('.'))
        .filter(|z| {
            //Note: FIXME: this filter implie that local zones are more restrictive than
            // the domain_filters configures
            filters.is_empty()
                || filters
                    .iter()
                    .map(|f| f.strip_prefix('.').unwrap_or(f))
                    .any(|f| z.ends_with(f))
        })
        .filter(|z| z.is_empty())
        .map(Into::into)
        .collect::<Vec<String>>();

    guard.extend(zones.clone());

    Ok(zones)
}
