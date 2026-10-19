mod print;

use std::sync::{Arc, Mutex};

use warp::body::BodyDeserializeError;
use warp::http::{Method, StatusCode};
use warp::Filter;
use warp::Reply;

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
) -> impl Filter<Extract = (impl warp::Reply,), Error = std::convert::Infallible> + Clone {
    print::print_routes(service, print_log)
        .with(cors())
        .recover(recover_json_rejection)
}

async fn recover_json_rejection(
    rejection: warp::Rejection,
) -> Result<impl Reply, std::convert::Infallible> {
    let (status, message) = if rejection.find::<warp::reject::PayloadTooLarge>().is_some() {
        (StatusCode::PAYLOAD_TOO_LARGE, "request body exceeds 4 MiB")
    } else if rejection.find::<BodyDeserializeError>().is_some() {
        (StatusCode::BAD_REQUEST, "malformed JSON request body")
    } else {
        (StatusCode::NOT_FOUND, "route not found")
    };

    Ok(warp::reply::with_status(
        warp::reply::json(&crate::models::StatusResponse::error(
            false,
            format!("Invalid input: {message}"),
        )),
        status,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::{PreparedPrint, PrintTransport, PrinterService};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct RecordingTransport {
        calls: AtomicUsize,
        jobs: Mutex<Vec<(bool, usize, usize)>>,
    }
    impl PrintTransport for RecordingTransport {
        fn execute(&self, job: &PreparedPrint) -> Result<(), crate::error::AppError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.jobs.lock().unwrap().push((
                job.reprint,
                job.commands.len(),
                job.special_bytes.len(),
            ));
            Ok(())
        }
    }

    fn test_routes() -> (
        impl Filter<Extract = (impl Reply,), Error = std::convert::Infallible> + Clone,
        Arc<RecordingTransport>,
    ) {
        let transport = Arc::new(RecordingTransport {
            calls: AtomicUsize::new(0),
            jobs: Mutex::new(Vec::new()),
        });
        let service = PrinterService::test_service(transport.clone());
        (
            routes(service, Arc::new(Mutex::new(PrintLog::new(10)))),
            transport,
        )
    }

    #[tokio::test]
    async fn print_and_reprint_route_matrix_rejects_invalid_input_without_transport() {
        let (api, transport) = test_routes();
        let malformed = warp::test::request()
            .method("POST")
            .path("/print")
            .body("{")
            .reply(&api)
            .await;
        assert_eq!(malformed.status(), 400);
        let unknown = warp::test::request()
            .method("POST")
            .path("/print")
            .json(&serde_json::json!({"commands":[{"command":"Unknown","parameters":{}}]}))
            .reply(&api)
            .await;
        assert_eq!(unknown.status(), 400);
        let invalid = warp::test::request().method("POST").path("/print/reprint").json(&serde_json::json!({"commands":[{"command":"Writeln","parameters":"prefix"},{"command":"Pdf417","parameters":"x".repeat(65_532)}]})).reply(&api).await;
        assert_eq!(invalid.status(), 400);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
        let valid = warp::test::request().method("POST").path("/print").header("origin", "https://example.test").json(&serde_json::json!({"commands":[{"command":"Writeln","parameters":"ok"},{"command":"CashDrawerPulse","parameters":{"pin":"Pin2","pulseMs":100}},{"command":"BitImage","parameters":{"width":8,"height":1,"data":"AA=="}},{"command":"Italic","parameters":true}]})).reply(&api).await;
        assert_eq!(valid.status(), 200);
        let reprint = warp::test::request()
            .method("POST")
            .path("/print/reprint")
            .json(&serde_json::json!({"commands":[{"command":"Writeln","parameters":"ok"}]}))
            .reply(&api)
            .await;
        assert_eq!(reprint.status(), 200);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
        let jobs = transport.jobs.lock().unwrap();
        assert_eq!(jobs[0], (false, 4, 2));
        assert!(jobs[1].0 && jobs[1].1 >= 3);
        assert!(valid.headers().get("access-control-allow-origin").is_some());
    }

    #[tokio::test]
    async fn oversized_body_is_413_for_both_print_endpoints() {
        let (api, _) = test_routes();
        let body = "x".repeat(crate::models::MAX_JOB_BODY_BYTES + 1);
        for path in ["/print", "/print/reprint"] {
            let response = warp::test::request()
                .method("POST")
                .path(path)
                .body(body.clone())
                .reply(&api)
                .await;
            assert_eq!(response.status(), 413, "{path}");
        }
    }

    #[tokio::test]
    async fn health_and_printer_status_are_read_only_fast_endpoints() {
        let (api, _) = test_routes();
        let health = warp::test::request()
            .method("GET")
            .path("/health")
            .reply(&api)
            .await;
        assert_eq!(health.status(), 200);
        let health_json: serde_json::Value = serde_json::from_slice(health.body()).unwrap();
        assert_eq!(health_json["status"], "ok");
        assert!(health_json["version"].is_string());
        assert!(health_json["uptime_seconds"].is_number());

        let status = warp::test::request()
            .method("GET")
            .path("/printer/status")
            .reply(&api)
            .await;
        assert_eq!(status.status(), 200);
        let status_json: serde_json::Value = serde_json::from_slice(status.body()).unwrap();
        assert!(status_json["transport_age_millis"].is_number());
        assert!(status_json["generation"].is_number());
    }
}
