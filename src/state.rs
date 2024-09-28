use crate::config::Config;
use crate::external_dns::Endpoint;
use crate::opnsense::unbound::HostOverrideRecord;
use crate::opnsense::Opnsense;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState<R: RecordCache, Z: ZoneCache> {
    pub config: Config,
    pub opnsense: Opnsense,
    pub record_cache: Arc<RwLock<R>>,
    pub zone_cache: Arc<RwLock<Z>>,
}

pub trait RecordCache {
    fn try_get_record(&self, record: &HostOverrideRecord) -> anyhow::Result<Option<RecordEntry>>;
    fn try_insert_record(
        &mut self,
        record: &HostOverrideRecord,
    ) -> anyhow::Result<Option<RecordEntry>>;
    fn try_remove_record(&mut self, record: &HostOverrideRecord) -> anyhow::Result<()>;
    fn clear(&mut self);
}

#[derive(Clone)]
pub struct DefaultRecordCache(HashMap<Recordkey, RecordEntry>);

impl DefaultRecordCache {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

impl RecordCache for DefaultRecordCache {
    fn try_get_record(&self, record: &HostOverrideRecord) -> anyhow::Result<Option<RecordEntry>> {
        Ok(self.0.get(&record.try_into()?).cloned())
    }
    fn try_insert_record(
        &mut self,
        record: &HostOverrideRecord,
    ) -> anyhow::Result<Option<RecordEntry>> {
        Ok(self.0.insert(record.try_into()?, record.try_into()?))
    }
    fn try_remove_record(&mut self, record: &HostOverrideRecord) -> anyhow::Result<()> {
        self.0.remove(&record.try_into()?);
        Ok(())
    }
    fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct Recordkey {
    pub fqdn: String,
    pub record_type: RecordType,
}

impl TryFrom<&HostOverrideRecord> for Recordkey {
    type Error = anyhow::Error;
    fn try_from(value: &HostOverrideRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            fqdn: format!("{}.{}", value.hostname, value.domain),
            record_type: value.rr.clone().try_into()?,
        })
    }
}

impl TryFrom<Endpoint> for Recordkey {
    type Error = anyhow::Error;
    fn try_from(value: Endpoint) -> Result<Self, Self::Error> {
        Ok(Self {
            fqdn: value.dns_name,
            record_type: value.record_type.try_into()?,
        })
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum RecordType {
    A,
    AAAA,
}

impl TryFrom<String> for RecordType {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let value = value
            .trim()
            .split_once(' ')
            .map(|v| v.0)
            .unwrap_or(&value)
            .to_uppercase();

        Ok(match value.as_str() {
            "A" => Self::A,
            "AAAA" => Self::AAAA,
            _ => Err(anyhow::anyhow!("unknown record type"))?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct RecordEntry {
    pub uuid: String,
    //pub server: String,
    pub enabled: bool,
}

impl TryFrom<&HostOverrideRecord> for RecordEntry {
    type Error = anyhow::Error;
    fn try_from(value: &HostOverrideRecord) -> Result<Self, Self::Error> {
        Ok(RecordEntry {
            uuid: value.uuid.clone(),
            //server: value.server.clone(),
            enabled: match value.enabled.trim() {
                "0" => false,
                "1" => true,
                _ => Err(anyhow::anyhow!("unknown enabled state"))?,
            },
        })
    }
}

pub trait ZoneCache {
    fn extend(&mut self, values: impl IntoIterator<Item = String>);
    fn values(&self) -> Vec<String>;
}

#[derive(Clone)]
pub struct DefaultZoneCache(HashSet<String>);

impl DefaultZoneCache {
    pub fn new() -> Self {
        Self(HashSet::new())
    }
}

impl ZoneCache for DefaultZoneCache {
    fn extend(&mut self, values: impl IntoIterator<Item = String>) {
        self.0.extend(values)
    }
    fn values(&self) -> Vec<String> {
        self.0.clone().into_iter().collect()
    }
}
