use opnsense_unbound_external_dns_webhook::{config::Config, Server};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let collector = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(collector)?;

    Server::from(Config::try_from_env()?).serve().await
}
