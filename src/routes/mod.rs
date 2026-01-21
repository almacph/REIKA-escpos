mod print;

use warp::http::Method;
use warp::Filter;

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
) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    print::print_routes(service).with(cors())
}
