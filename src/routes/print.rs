use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use warp::Filter;

use crate::app::PrintLog;
use crate::handlers::{handle_print, handle_status, handle_test_print};
use crate::models::PrinterTestSchema;
use crate::services::PrinterService;

fn with_service(
    service: PrinterService,
) -> impl Filter<Extract = (PrinterService,), Error = Infallible> + Clone {
    warp::any().map(move || service.clone())
}

fn with_print_log(
    print_log: Arc<Mutex<PrintLog>>,
) -> impl Filter<Extract = (Arc<Mutex<PrintLog>>,), Error = Infallible> + Clone {
    warp::any().map(move || print_log.clone())
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
    print_log: Arc<Mutex<PrintLog>>,
) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    warp::path!("print" / "test")
        .and(warp::post())
        .and(with_service(service))
        .and(with_print_log(print_log))
        .and(warp::body::json::<PrinterTestSchema>())
        .and_then(
            |service: PrinterService,
             print_log: Arc<Mutex<PrintLog>>,
             request: PrinterTestSchema| async move {
                handle_test_print(service, print_log, request).await
            },
        )
}

pub fn print_route(
    service: PrinterService,
    print_log: Arc<Mutex<PrintLog>>,
) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    warp::path("print")
        .and(warp::path::end())
        .and(warp::post())
        .and(with_service(service))
        .and(with_print_log(print_log))
        .and(warp::body::json())
        .and_then(
            |service: PrinterService,
             print_log: Arc<Mutex<PrintLog>>,
             body: serde_json::Value| async move { handle_print(service, print_log, body).await },
        )
}

pub fn print_routes(
    service: PrinterService,
    print_log: Arc<Mutex<PrintLog>>,
) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    status_route(service.clone())
        .or(test_print_route(service.clone(), print_log.clone()))
        .or(print_route(service, print_log))
}
