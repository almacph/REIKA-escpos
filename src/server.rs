use std::net::SocketAddr;
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::app::PrintLog;
use crate::routes::routes;
use crate::services::PrinterService;

static SERVER_STARTED: OnceLock<Instant> = OnceLock::new();

pub fn mark_started() {
    let _ = SERVER_STARTED.set(Instant::now());
}

pub fn uptime_seconds() -> u64 {
    SERVER_STARTED.get_or_init(Instant::now).elapsed().as_secs()
}

async fn bind_http_listener(port: u16) -> Result<tokio::net::TcpListener, String> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("could not bind HTTP server to 127.0.0.1:{port}: {error}"))
}

pub async fn run_with_port(
    service: PrinterService,
    print_log: Arc<Mutex<PrintLog>>,
    port: u16,
    state_tx: tokio::sync::watch::Sender<String>,
) -> Result<(), String> {
    let routes = routes(service, print_log);
    let listener = match bind_http_listener(port).await {
        Ok(listener) => listener,
        Err(error) => {
            let _ = state_tx.send(format!("Failed: {error}"));
            return Err(error);
        }
    };
    let _ = state_tx.send(format!("Listening on 127.0.0.1:{port}"));
    log::info!("HTTP server listening on 127.0.0.1:{port}");
    warp::serve(routes).incoming(listener).run().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::bind_http_listener;

    #[tokio::test]
    async fn bind_failure_is_returned_with_address() {
        let occupied = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test listener should bind");
        let port = occupied.local_addr().expect("listener address").port();

        let error = bind_http_listener(port)
            .await
            .expect_err("occupied port must be reported");
        assert!(error.contains(&format!("127.0.0.1:{port}")));
    }
}
