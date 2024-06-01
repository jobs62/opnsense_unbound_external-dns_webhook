use figment::{
    providers::{Env, Format, Yaml},
    Figment,
};
use serde::{de, Deserialize, Deserializer};

#[derive(Clone, Deserialize, Debug)]
pub struct Config {
    pub key: String,
    pub secret: String,
    #[serde(deserialize_with = "from_str_deserialize")]
    pub base: reqwest::Url,
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default)]
    pub domain_filters: Vec<String>,
    #[serde(default)]
    pub allow_invalid_certs: bool,
    #[serde(deserialize_with = "deserialize_certificate", default)]
    pub certificate_bundle: Vec<reqwest::Certificate>,
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

fn default_bind() -> String {
    "127.0.0.1:8800".to_owned()
}

fn deserialize_certificate<'de, D>(deserializer: D) -> Result<Vec<reqwest::Certificate>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match Option::<String>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(b) => {
            reqwest::Certificate::from_pem_bundle(b.as_bytes()).map_err(de::Error::custom)?
        }
    })
}

impl Config {
    pub fn try_from_env() -> figment::Result<Config> {
        Figment::new()
            .merge(Yaml::file("config.yaml"))
            .merge(Env::prefixed("OPNSENSE_"))
            .extract()
    }
}
