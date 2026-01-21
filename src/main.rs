mod error;
mod handlers;
mod models;
mod routes;
mod server;
mod services;

use crate::server::run;
use crate::services::PrinterService;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let driver = PrinterService::initialize_device().await;
    let service = PrinterService::new(driver);

    run(service).await;
}
