use opto_sync_gateway::{router, Config};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("opto-sync-gateway startup failed: {error}");
        std::process::exit(2);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let bind = config.bind;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, router(config))
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}
