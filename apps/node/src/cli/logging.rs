use eyre::eyre;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{
    EnvFilter, Layer,
    fmt::format::{DefaultVisitor, Writer},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

pub fn init(level: Level) -> eyre::Result<()> {
    let stdout_filter = new_env_filter(level, "RUST_LOG")?;

    tracing_subscriber::registry()
        .with(ZkMessengerTracer.with_filter(stdout_filter))
        .try_init()?;

    Ok(())
}

fn new_env_filter(level: Level, env_var: &str) -> eyre::Result<EnvFilter> {
    let default_directives = [
        format!("{}", level),
        "hyper_util=info".to_string(),
        "hyper=info".to_string(),
        "h2=info".to_string(),
    ];

    let env_directives = std::env::var(env_var).ok();

    let filter = match env_directives {
        Some(env) => {
            let mut filter = EnvFilter::new("");
            for directive in default_directives {
                filter = filter.add_directive(
                    directive
                        .parse()
                        .map_err(|_| eyre!("Invalid directive: {}", directive))?,
                )
            }

            // Add the directives from the environment variable, which should override all of our
            // defaults since they're being added last.
            if !env.is_empty() {
                for directive in env.split(',') {
                    filter = filter.add_directive(
                        directive
                            .parse()
                            .map_err(|_| eyre!("Invalid directive: {}", directive))?,
                    )
                }
            }

            filter
        }
        None => EnvFilter::new(default_directives.join(",")),
    };

    Ok(filter)
}

pub struct ZkMessengerTracer;

impl<S> Layer<S> for ZkMessengerTracer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let target = match event.metadata().level() {
            &Level::INFO | &Level::WARN | &Level::ERROR => event
                .metadata()
                .target()
                .split("::")
                .last()
                .unwrap_or_default(),
            _ => event.metadata().target(),
        };

        print!(
            "[{}] {} {}: ",
            chrono::offset::Local::now().format("%Y-%m-%d %H:%M:%S"),
            event.metadata().level(),
            target,
        );

        let mut message = String::new();

        event.record(&mut DefaultVisitor::new(Writer::new(&mut message), true));

        println!("{}", message);
    }
}
