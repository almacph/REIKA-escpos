use std::convert::Infallible;

use warp::http::StatusCode;
use warp::reply::json;
use warp::Reply;

use crate::models::{Commands, PrinterTestSchema, StatusResponse};
use crate::services::PrinterService;

pub async fn handle_status(service: PrinterService) -> Result<impl Reply, Infallible> {
    let is_connected = service.check_connection().await;

    let response = if is_connected {
        StatusResponse::success()
    } else {
        StatusResponse::disconnected(
            "The thermal printer is either not plugged in, or is in a not ready state.",
        )
    };

    println!(
        "{} sent!",
        if is_connected {
            "Connected"
        } else {
            "Not connected"
        }
    );

    Ok(warp::reply::with_status(json(&response), StatusCode::OK))
}

pub async fn handle_test_print(
    service: PrinterService,
    request: PrinterTestSchema,
) -> Result<impl Reply, Infallible> {
    match service.print_test(request).await {
        Ok(()) => Ok(warp::reply::with_status(
            json(&StatusResponse::success()),
            StatusCode::OK,
        )),
        Err(e) => {
            let status = e.status_code();
            Ok(warp::reply::with_status(
                json(&e.to_response(false)),
                status,
            ))
        }
    }
}

pub async fn handle_print(
    service: PrinterService,
    body: serde_json::Value,
) -> Result<impl Reply, Infallible> {
    let json_string = serde_json::to_string(&body).unwrap_or_default();
    println!("Parsing a print request! {:#?}", json_string);

    let commands: Commands = match serde_json::from_value(body) {
        Ok(c) => {
            println!("{:?}", c);
            c
        }
        Err(e) => {
            println!("Failed to parse the JSON for the previous print request!");
            return Ok(warp::reply::with_status(
                json(&StatusResponse::error(false, format!("Invalid input: {}", e))),
                StatusCode::BAD_REQUEST,
            ));
        }
    };

    match service.execute_commands(commands).await {
        Ok(()) => Ok(warp::reply::with_status(
            json(&StatusResponse::success()),
            StatusCode::OK,
        )),
        Err(e) => {
            let status = e.status_code();
            Ok(warp::reply::with_status(
                json(&e.to_response(false)),
                status,
            ))
        }
    }
}
