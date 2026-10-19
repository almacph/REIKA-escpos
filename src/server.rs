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
    use super::{bind_http_listener, run_with_port};
    use crate::app::PrintLog;
    use crate::services::{PrinterService, UsbConfig};
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

    #[tokio::test]
    async fn http_server_bootstrap_serves_health_over_tcp() {
        let reservation = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test reservation should bind");
        let port = reservation
            .local_addr()
            .expect("reservation address")
            .port();
        drop(reservation);

        let service = PrinterService::unavailable(UsbConfig {
            vendor_id: 0,
            product_id: 0,
            endpoint: None,
            interface: None,
        });
        let print_log = Arc::new(Mutex::new(PrintLog::new(4)));
        let (state_tx, mut state_rx) = tokio::sync::watch::channel("Starting".to_string());
        let server = tokio::spawn(run_with_port(service, print_log, port, state_tx));

        tokio::time::timeout(std::time::Duration::from_secs(1), state_rx.changed())
            .await
            .expect("HTTP startup state should be published")
            .expect("state channel should remain open");
        assert!(state_rx.borrow().starts_with("Listening"));

        let mut client = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("HTTP listener should accept connections");
        client
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .expect("health request should be written");
        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("health response should be readable");
        let response = String::from_utf8_lossy(&response);
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.contains("\"status\":\"ok\""));

        server.abort();
        let _ = server.await;
    }
}
