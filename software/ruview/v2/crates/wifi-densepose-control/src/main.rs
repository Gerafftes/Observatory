#[tokio::main]
async fn main() {
    if let Err(error) = wifi_densepose_control::server::run().await {
        eprintln!("RuView-Control konnte nicht gestartet werden: {error}");
        std::process::exit(1);
    }
}
