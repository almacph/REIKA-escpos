use std::convert::Infallible;

use escpos::{driver::UsbDriver, errors::PrinterError};
use warp::{http::Method, http::StatusCode, Filter, reply::json};

use crate::{models::{parse_json, PrinterTestSchema, StatusResponse}, print::{handle_test_print, is_device_connected, print_receipt}};

pub async fn run( driver: UsbDriver) {
    let routes = routes(driver);
    println!("Serving the server!");
    warp::serve(routes).run(([127, 0, 0, 1], 55000)).await;
    
}

pub fn routes( driver: UsbDriver) -> impl Filter<Extract =  impl warp::Reply, Error = warp::Rejection> + Clone {
    print_route(driver.clone()).or(receipt_route(driver))
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


pub fn receipt_route(driver: UsbDriver) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    print(driver).with(cors())
}

fn print(driver: UsbDriver) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("print")
        .and(warp::path::end())
        .and(warp::post())
        .and(with_driver(driver))
        .and(warp::body::json())
        .and_then(handle_request)
}


async fn handle_request(driver: UsbDriver, json_body: serde_json::Value) -> Result<impl warp::Reply, Infallible> {
    let json_string = serde_json::to_string(&json_body).unwrap();
    match print_middleman(driver, &json_string).await {
        Ok(_) => Ok(warp::reply::with_status("Printed successfully", StatusCode::OK)),
        Err(e) => {
            let response = match e {
                PrinterError::Input(_) => {
                    println!("Failed to parse the JSON for the previous print request!");
                    warp::reply::with_status("Failed to parse the JSON.", StatusCode::BAD_REQUEST)
                },
                PrinterError::InvalidResponse(_) => {
                    warp::reply::with_status("Failed to print: Invalid Response.", StatusCode::BAD_GATEWAY)
                },
                PrinterError::Io(_) => {
                    warp::reply::with_status("Failed to print: IO Error", StatusCode::INTERNAL_SERVER_ERROR)
                },
            };
            Ok(response)
        }
    }
}

async fn print_middleman(driver: UsbDriver, json_commands: &str) -> Result<(), PrinterError> {
    println!("print_middleman");
    match parse_json(json_commands) {
        Ok(_) => {
            // Continue execution if parsing was successful
            print_receipt(driver, json_commands).await.map_err(|e| {
                // Map your specific error here based on the context of the error
                match e {
                    PrinterError::Input(error) => PrinterError::Input(error),
                    PrinterError::InvalidResponse(error) => PrinterError::InvalidResponse(error),
                    PrinterError::Io(error) => PrinterError::Io(error),
                }
            })
        },
        Err(e) => {
            // Return the parsing error
            Err(e)
        }
    }
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
