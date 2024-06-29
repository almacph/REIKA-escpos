use escpos::{driver::UsbDriver};
use warp::{http::Method, http::StatusCode, Filter, reply::json};

use crate::{models::{PrinterTestSchema, StatusResponse}, print::{handle_test_print, is_device_connected}};

pub async fn run( driver: UsbDriver) {
    let routes = routes(driver);
    println!("Serving the server!");
    warp::serve(routes).run(([127, 0, 0, 1], 55000)).await;
    
}

pub fn routes( driver: UsbDriver) -> impl Filter<Extract =  impl warp::Reply, Error = warp::Rejection> + Clone {
    print_route(driver)
}

fn with_driver(
    driver: UsbDriver,
) -> impl Filter<Extract = (UsbDriver,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || driver.clone())
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

pub fn print_route( driver: UsbDriver) -> impl Filter<Extract =  impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!("print" / "test").and(test(driver.clone()).or(status(driver))).with(cors())
}

fn test(driver: UsbDriver) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path::end()
        .and(warp::post())
        .and(with_driver(driver.clone()))
        .and(warp::body::json::<PrinterTestSchema>())
        .and_then(|driver: UsbDriver, print_request:PrinterTestSchema| async move {
            match handle_test_print(driver, print_request).await {
                Ok(_) => Ok::<_, warp::Rejection>(warp::reply::with_status("Printed successfully", StatusCode::OK)),
                Err(_) => Err(warp::reject::reject()),
            }
        })
}

fn status(driver: UsbDriver) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path::end()
        .and(warp::get())
        .and(with_driver(driver.clone()))
        .and_then(|driver: UsbDriver| async move {
            status_handler(driver).await
        })
        .boxed()
}

async fn status_handler(driver: UsbDriver) -> Result<impl warp::Reply, warp::Rejection> {
    let is_connected = is_device_connected(driver).await;
    if is_connected {
        println!("Connected sent!");
        Ok(warp::reply::with_status(
            json(&StatusResponse {
                is_connected,
                error: "Printer is connected".to_string(),
            }),
            StatusCode::OK,
        ))
    } else {
        println!("Not connected sent!");
        Ok(warp::reply::with_status(
            json(&StatusResponse {
                is_connected,
                error: "The thermal printer is either not plugged in, or is in a not ready state.".to_string(),
            }),
            StatusCode::OK,
        ))
    }
}
