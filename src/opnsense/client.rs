use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::config::Config;
use crate::opnsense::Result;

#[derive(Clone)]
pub struct Client {
    auth: ClientAuth,
    client: reqwest::Client,
    base_url: reqwest::Url,
}

#[derive(Clone)]
struct ClientAuth {
    key: String,
    secret: String,
}

impl TryFrom<&Config> for Client {
    type Error = anyhow::Error;

    fn try_from(config: &Config) -> std::result::Result<Self, Self::Error> {
        let mut builder =
            reqwest::Client::builder().danger_accept_invalid_certs(config.allow_invalid_certs);

        for c in config.certificate_bundle.iter() {
            builder = builder.add_root_certificate(c.clone());
        }

        Ok(Self {
            auth: ClientAuth {
                key: config.key.clone(),
                secret: config.secret.clone(),
            },
            client: builder.build()?,
            base_url: config.base.join("api/")?,
        })
    }
}

impl Client {
    pub fn with_path(&self, path: &str) -> Result<Self> {
        Ok(Self {
            base_url: self.base_url.join(path)?,
            ..self.clone()
        })
    }
    pub async fn get<M: Method>(&self, path: &str) -> Result<M::Response> {
        Ok(self
            .client
            .get(self.base_url.join(path).unwrap())
            .basic_auth(&self.auth.key, Some(&self.auth.secret))
            .send()
            .await
            .and_then(|r| r.error_for_status())?
            .json::<M::Response>()
            .await?)
    }
    pub async fn post<M: Method>(&self, path: &str, json: Value) -> Result<M::Response> {
        Ok(self
            .client
            .post(self.base_url.join(path).unwrap())
            .basic_auth(&self.auth.key, Some(&self.auth.secret))
            .json(&json)
            .send()
            .await
            .and_then(|r| r.error_for_status())?
            .json::<M::Response>()
            .await?)
    }
}

pub trait Method {
    type Response: DeserializeOwned;
}
