use crate::routes::routes;
use crate::services::PrinterService;

pub const DEFAULT_PORT: u16 = 55000;

pub async fn run(service: PrinterService) {
    run_with_port(service, DEFAULT_PORT).await;
}

pub async fn run_with_port(service: PrinterService, port: u16) {
    let routes = routes(service);
    println!("Serving the server on 127.0.0.1:{}", port);
    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}
