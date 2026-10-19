use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use warp::http::StatusCode;
use warp::reply::json;
use warp::Reply;

use crate::app::file_logger::record_diagnostic_event;
use crate::app::{notify_print_error, notify_print_success, PrintLog};
use crate::models::{
    Commands, HealthResponse, PrinterTestSchema, StatusResponse, MAX_JOB_BODY_BYTES,
};
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

    Ok(warp::reply::with_status(json(&response), StatusCode::OK))
}

pub async fn handle_health() -> Result<impl Reply, Infallible> {
    let response = HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds: crate::server::uptime_seconds(),
    };
    Ok(warp::reply::with_status(json(&response), StatusCode::OK))
}

pub async fn handle_printer_status(service: PrinterService) -> Result<impl Reply, Infallible> {
    let snapshot = service.health_snapshot();
    Ok(warp::reply::with_status(json(&snapshot), StatusCode::OK))
}

pub async fn handle_test_print(
    service: PrinterService,
    print_log: Arc<Mutex<PrintLog>>,
    request: PrinterTestSchema,
) -> Result<impl Reply, Infallible> {
    match service.print_test(request).await {
        Ok(()) => {
            record_diagnostic_event("print_succeeded", "print_attempt", None);
            notify_print_success("Test print");
            if let Ok(mut log) = print_log.lock() {
                log.add_success("Test print".to_string());
            }
            Ok(warp::reply::with_status(
                json(&StatusResponse::success()),
                StatusCode::OK,
            ))
        }
        Err(e) => {
            record_diagnostic_event("print_failed", "print_attempt", None);
            let status = e.status_code();
            let error_msg = e.to_string();
            notify_print_error("Test print", &error_msg);
            if let Ok(mut log) = print_log.lock() {
                log.add_error("Test print".to_string(), error_msg);
            }
            Ok(warp::reply::with_status(
                json(&e.to_response(false)),
                status,
            ))
        }
    }
}

pub async fn handle_print(
    service: PrinterService,
    print_log: Arc<Mutex<PrintLog>>,
    body: serde_json::Value,
) -> Result<impl Reply, Infallible> {
    if serde_json::to_vec(&body).map_or(true, |bytes| bytes.len() > MAX_JOB_BODY_BYTES) {
        return Ok(warp::reply::with_status(
            json(&StatusResponse::error(
                false,
                "Invalid input: print job exceeds 4 MiB",
            )),
            StatusCode::BAD_REQUEST,
        ));
    }
    let commands: Commands = match serde_json::from_value(body) {
        Ok(c) => c,
        Err(e) => {
            let error_msg = format!("Invalid input: {}", e);
            if let Ok(mut log) = print_log.lock() {
                log.add_error("Print job".to_string(), error_msg.clone());
            }
            return Ok(warp::reply::with_status(
                json(&StatusResponse::error(false, error_msg)),
                StatusCode::BAD_REQUEST,
            ));
        }
    };

    if let Err(error) = commands.validate() {
        let error_msg = format!("Invalid input: {error}");
        if let Ok(mut log) = print_log.lock() {
            log.add_error("Print job".to_string(), error_msg.clone());
        }
        return Ok(warp::reply::with_status(
            json(&StatusResponse::error(false, error_msg)),
            StatusCode::BAD_REQUEST,
        ));
    }

    let cmd_count = commands.commands.len();
    let commands_for_log = commands.commands.clone();
    match service.execute_commands(commands).await {
        Ok(()) => {
            record_diagnostic_event("print_succeeded", "print_attempt", None);
            let summary = format!("Print job ({} commands)", cmd_count);
            notify_print_success(&summary);
            if let Ok(mut log) = print_log.lock() {
                log.add_success_with_commands(summary, commands_for_log);
            }
            Ok(warp::reply::with_status(
                json(&StatusResponse::success()),
                StatusCode::OK,
            ))
        }
        Err(e) => {
            record_diagnostic_event("print_failed", "print_attempt", None);
            let status = e.status_code();
            let error_msg = e.to_string();
            notify_print_error("Print job", &error_msg);
            if let Ok(mut log) = print_log.lock() {
                log.add_error_with_commands(
                    format!("Print job ({} commands)", cmd_count),
                    error_msg,
                    commands_for_log,
                );
            }
            Ok(warp::reply::with_status(
                json(&e.to_response(false)),
                status,
            ))
        }
    }
}

pub async fn handle_reprint(
    service: PrinterService,
    body: serde_json::Value,
) -> Result<impl Reply, Infallible> {
    if serde_json::to_vec(&body).map_or(true, |bytes| bytes.len() > MAX_JOB_BODY_BYTES) {
        return Ok(warp::reply::with_status(
            json(&StatusResponse::error(
                false,
                "Invalid input: reprint job exceeds 4 MiB",
            )),
            StatusCode::BAD_REQUEST,
        ));
    }
    let commands: Commands = match serde_json::from_value(body) {
        Ok(c) => c,
        Err(e) => {
            let error_msg = format!("Invalid input: {}", e);
            return Ok(warp::reply::with_status(
                json(&StatusResponse::error(false, error_msg)),
                StatusCode::BAD_REQUEST,
            ));
        }
    };

    if let Err(error) = commands.validate() {
        return Ok(warp::reply::with_status(
            json(&StatusResponse::error(
                false,
                format!("Invalid input: {error}"),
            )),
            StatusCode::BAD_REQUEST,
        ));
    }

    match service.execute_reprint_commands(commands).await {
        Ok(()) => {
            notify_print_success("Reprint");
            Ok(warp::reply::with_status(
                json(&StatusResponse::success()),
                StatusCode::OK,
            ))
        }
        Err(e) => {
            let status = e.status_code();
            let error_msg = e.to_string();
            notify_print_error("Reprint", &error_msg);
            Ok(warp::reply::with_status(
                json(&e.to_response(false)),
                status,
            ))
        }
    }
}
