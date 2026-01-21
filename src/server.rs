use crate::routes::routes;
use crate::services::PrinterService;

pub async fn run(service: PrinterService) {
    let routes = routes(service);
    println!("Serving the server on 127.0.0.1:55000");
    warp::serve(routes).run(([127, 0, 0, 1], 55000)).await;
}
