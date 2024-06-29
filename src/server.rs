use escpos::{driver::UsbDriver};
use warp::{http::Method, http::StatusCode, Filter};
use serde_json::json;

use crate::{models::PrinterTestSchema, print::handle_test_print};

pub async fn run( driver: UsbDriver) {
    let routes = routes(driver);
    println!("Serving the server!");
    warp::serve(routes).run(([127, 0, 0, 1], 55000)).await;
    
}

pub fn routes( driver: UsbDriver) -> impl Filter<Extract =  impl warp::Reply, Error = warp::Rejection> + Clone {
    hello_route().or(print_route(driver))
}

fn cors() -> warp::cors::Cors {
    warp::cors()
        .allow_any_origin()
        .allow_methods(vec![Method::GET, Method::POST])
        .allow_headers(vec![
            "Content-Type",
            "Authorization",
            "Accept",
            "Origin",
        ])
        .build()
}


fn hello_route() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path::end().map(|| warp::reply::json(&json!("Hello Word!")))
}

fn print_route(driver: UsbDriver) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!("print" / "test")
        .and(warp::post())
        .and(with_driver(driver.clone()))
        .and(warp::body::json::<PrinterTestSchema>())
        .and_then(|driver: UsbDriver, print_request:PrinterTestSchema| async move {
            match handle_test_print(driver, print_request).await {
                Ok(_) => Ok::<_, warp::Rejection>(warp::reply::with_status("Printed successfully", StatusCode::OK)),
                Err(_) => Err(warp::reject::reject()),
            }
        }).with(cors())
}

fn with_driver(
    driver: UsbDriver,
) -> impl Filter<Extract = (UsbDriver,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || driver.clone())
}