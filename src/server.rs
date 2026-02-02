use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app::PrintLog;
use crate::routes::routes;
use crate::services::PrinterService;

const HEALTH_CHECK_INTERVAL_SECS: u64 = 30;

pub async fn run_with_port(service: PrinterService, print_log: Arc<Mutex<PrintLog>>, port: u16) {
    let health_service = service.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS)).await;
            health_service.check_connection().await;
        }
    });

    let routes = routes(service, print_log);
    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}
