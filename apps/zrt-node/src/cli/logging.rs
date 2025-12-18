use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt, util::SubscriberInitExt};
use types::utils::LocalTimer;

pub fn init() -> eyre::Result<()> {
    Registry::default()
        .with(EnvFilter::from_default_env())
        .with(
            tracing_subscriber::fmt::layer()
                .compact()
                // .pretty()
                .with_timer(LocalTimer)
                .with_target(false)
                .with_ansi(true),
        )
        .try_init()?;

    Ok(())
}
