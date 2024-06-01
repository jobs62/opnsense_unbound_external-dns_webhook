use crate::config::Config;
mod client;
pub mod unbound;

use client::Client;
use unbound::Unbound;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone)]
pub struct Opnsense {
    client: Client,
}

impl Opnsense {
    pub fn unbound(&self) -> Unbound {
        Unbound::new(self.client.clone())
    }
}

impl TryFrom<&Config> for Opnsense {
    type Error = Box<dyn std::error::Error>;

    fn try_from(config: &Config) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            client: Client::try_from(config)?,
        })
    }
}
