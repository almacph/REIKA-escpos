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

pub async fn run_with_port(service: PrinterService, print_log: Arc<Mutex<PrintLog>>, port: u16) {
    let routes = routes(service, print_log);
    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}
