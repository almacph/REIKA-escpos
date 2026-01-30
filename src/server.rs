use std::sync::{Arc, Mutex};

use crate::app::PrintLog;
use crate::routes::routes;
use crate::services::PrinterService;

pub const DEFAULT_PORT: u16 = 55000;

pub async fn run(service: PrinterService, print_log: Arc<Mutex<PrintLog>>) {
    run_with_port(service, print_log, DEFAULT_PORT).await;
}

pub async fn run_with_port(service: PrinterService, print_log: Arc<Mutex<PrintLog>>, port: u16) {
    let routes = routes(service, print_log);
    println!("Serving the server on 127.0.0.1:{}", port);
    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}
