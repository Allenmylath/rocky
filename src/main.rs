mod agent;
mod app;
mod fs;
mod serve;
mod state;
mod ui;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("rocky=debug".parse().unwrap()),
        )
        .init();

    app::run();
}