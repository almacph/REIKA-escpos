use std::convert::Infallible;

use warp::Filter;

use crate::handlers::{handle_print, handle_status, handle_test_print};
use crate::models::PrinterTestSchema;
use crate::services::PrinterService;

fn with_service(
    service: PrinterService,
) -> impl Filter<Extract = (PrinterService,), Error = Infallible> + Clone {
    warp::any().map(move || service.clone())
}

pub fn status_route(
    service: PrinterService,
) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    warp::path!("print" / "test")
        .and(warp::get())
        .and(with_service(service))
        .and_then(handle_status)
}

pub fn test_print_route(
    service: PrinterService,
) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    warp::path!("print" / "test")
        .and(warp::post())
        .and(with_service(service))
        .and(warp::body::json::<PrinterTestSchema>())
        .and_then(|service: PrinterService, request: PrinterTestSchema| async move {
            handle_test_print(service, request).await
        })
}

pub fn print_route(
    service: PrinterService,
) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    warp::path("print")
        .and(warp::path::end())
        .and(warp::post())
        .and(with_service(service))
        .and(warp::body::json())
        .and_then(|service: PrinterService, body: serde_json::Value| async move {
            handle_print(service, body).await
        })
}

pub fn print_routes(
    service: PrinterService,
) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    status_route(service.clone())
        .or(test_print_route(service.clone()))
        .or(print_route(service))
}
