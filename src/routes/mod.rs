mod print;

use std::sync::{Arc, Mutex};

use warp::http::Method;
use warp::Filter;

use crate::app::PrintLog;
use crate::services::PrinterService;

pub fn cors() -> warp::cors::Cors {
    warp::cors()
        .allow_any_origin()
        .allow_methods(vec![Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(vec!["Content-Type", "Authorization", "Accept", "Origin"])
        .build()
}

pub fn routes(
    service: PrinterService,
    print_log: Arc<Mutex<PrintLog>>,
) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    print::print_routes(service, print_log).with(cors())
}
