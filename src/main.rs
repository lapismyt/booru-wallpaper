mod cli;
mod config;
mod fetch;
mod rating;
mod types;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    tokio::select! {
        r = cli::run() => {
            r?
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Stopping...")
        }
    };

    Ok(())
}
