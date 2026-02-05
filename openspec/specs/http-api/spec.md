# HTTP API Specification

RESTful API for thermal printer operations using Warp framework.

## Requirements

### Requirement: Server Binding

The system SHALL bind HTTP server to localhost only on configurable port.

**Implementation:** `src/server.rs:15-25`

```rust
pub async fn run_with_port(service: PrinterService, print_log: Arc<Mutex<PrintLog>>, port: u16) {
    // Health check task (every 30 seconds)
    let health_service = service.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            health_service.check_connection().await;
        }
    });

    let routes = routes(service, print_log);
    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}
```

#### Scenario: Server starts on configured port
- **WHEN** service starts with port 55000 (default)
- **THEN** HTTP server listens on `127.0.0.1:55000`

#### Scenario: Background health check
- **WHEN** server is running
- **THEN** health check runs every 30 seconds to verify printer connection

#### Scenario: Localhost-only binding
- **WHEN** external network attempts connection
- **THEN** connection is refused (security: localhost only)

---

### Requirement: CORS Configuration

The system SHALL allow cross-origin requests for browser-based consumers.

**Implementation:** `src/routes/mod.rs:15-22`

```rust
pub fn cors() -> warp::cors::Cors {
    warp::cors()
        .allow_any_origin()
        .allow_methods(vec![Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(vec!["Content-Type", "Authorization", "Accept", "Origin"])
        .build()
}
```

#### Scenario: REIKA browser app makes request
- **WHEN** SvelteKit app at `http://localhost:5173` calls API
- **THEN** CORS headers allow the request (`Access-Control-Allow-Origin: *`)

#### Scenario: Preflight OPTIONS request
- **WHEN** browser sends OPTIONS preflight
- **THEN** server responds with allowed methods and headers

---

### Requirement: Status Check Endpoint (GET /print/test)

The system SHALL provide endpoint to check printer connection status.

**Implementation:** `src/handlers/print.rs:11-23`, `src/routes/print.rs:30-36`

```rust
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

// Route definition
warp::path!("print" / "test")
    .and(warp::get())
    .and(with_service(service))
    .and_then(handle_status)
```

#### Scenario: Printer connected
- **WHEN** GET `/print/test` and printer is online
- **THEN** returns `200 OK` with `{"is_connected": true}`

#### Scenario: Printer disconnected
- **WHEN** GET `/print/test` and printer is offline
- **THEN** returns `200 OK` with `{"is_connected": false, "error": "The thermal printer is either not plugged in, or is in a not ready state."}`

---

### Requirement: Test Print Endpoint (POST /print/test)

The system SHALL provide endpoint to execute test print operations.

**Implementation:** `src/handlers/print.rs:25-55`, `src/routes/print.rs:38-57`

```rust
pub async fn handle_test_print(
    service: PrinterService,
    print_log: Arc<Mutex<PrintLog>>,
    request: PrinterTestSchema,
) -> Result<impl Reply, Infallible> {
    match service.print_test(request).await {
        Ok(()) => {
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
```

#### Scenario: Test page print
- **WHEN** POST `/print/test` with `{"test_page": true, "test_line": ""}`
- **THEN** prints full test pattern (bold, underline, reverse, sizes) and returns `200 OK`

#### Scenario: Custom test line
- **WHEN** POST `/print/test` with `{"test_page": false, "test_line": "Hello World"}`
- **THEN** prints "Hello World" with cut and returns `200 OK`

#### Scenario: Test print logged
- **WHEN** test print completes (success or error)
- **THEN** entry added to print_log.json with timestamp and status

---

### Requirement: Print Commands Endpoint (POST /print)

The system SHALL provide endpoint to execute arbitrary ESC/POS commands.

**Implementation:** `src/handlers/print.rs:57-102`, `src/routes/print.rs:59-76`

```rust
pub async fn handle_print(
    service: PrinterService,
    print_log: Arc<Mutex<PrintLog>>,
    body: serde_json::Value,
) -> Result<impl Reply, Infallible> {
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

    let cmd_count = commands.commands.len();
    let commands_for_log = commands.commands.clone();
    match service.execute_commands(commands).await {
        Ok(()) => {
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
```

#### Scenario: Valid print request
- **WHEN** POST `/print` with valid commands JSON
- **THEN** executes commands and returns `200 OK` with `{"is_connected": true}`

#### Scenario: Invalid JSON
- **WHEN** POST `/print` with malformed JSON
- **THEN** returns `400 BAD_REQUEST` with `{"is_connected": false, "error": "Invalid input: ..."}`

#### Scenario: Print with commands logged
- **WHEN** print job completes successfully
- **THEN** log entry includes command list for receipt preview in GUI

---

### Requirement: Response Schema

The system SHALL use consistent StatusResponse for all endpoints.

**Implementation:** `src/models/response.rs`

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StatusResponse {
    pub is_connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl StatusResponse {
    pub fn success() -> Self {
        Self { is_connected: true, error: None }
    }

    pub fn disconnected(error: impl Into<String>) -> Self {
        Self { is_connected: false, error: Some(error.into()) }
    }

    pub fn error(is_connected: bool, error: impl Into<String>) -> Self {
        Self { is_connected, error: Some(error.into()) }
    }
}
```

#### Scenario: Success response (error omitted)
- **WHEN** operation succeeds
- **THEN** response is `{"is_connected": true}` (no `error` field)

#### Scenario: Error response (error included)
- **WHEN** operation fails
- **THEN** response is `{"is_connected": false, "error": "message"}`

---

### Requirement: Error Status Code Mapping

The system SHALL map AppError variants to appropriate HTTP status codes.

**Implementation:** `src/error.rs:15-35`

```rust
#[derive(Debug)]
pub enum AppError {
    InvalidInput(String),
    PrinterError(String),
    Internal(String),
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::InvalidInput(_) => StatusCode::BAD_REQUEST,           // 400
            AppError::PrinterError(_) | AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR, // 500
        }
    }

    pub fn to_response(&self, is_connected: bool) -> StatusResponse {
        StatusResponse::error(is_connected, self.to_string())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            AppError::PrinterError(msg) => write!(f, "Printer error: {}", msg),
            AppError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}
```

#### Scenario: Invalid input error
- **WHEN** request has invalid JSON or parameters
- **THEN** returns HTTP 400 with `"Invalid input: {details}"`

#### Scenario: Printer error
- **WHEN** printer operation fails after retries
- **THEN** returns HTTP 500 with `"Printer error: {details}"`

---

### Requirement: System Notifications on Print Events

The system SHALL trigger Windows toast notifications for print outcomes.

**Implementation:** `src/app/notifications.rs`

```rust
pub fn notify_print_success(summary: &str) {
    let _ = Notification::new()
        .appname("REIKA Printer Service")
        .summary("Print Completed")
        .body(&format!("{} printed successfully", summary))
        .timeout(3000)
        .show();
}

pub fn notify_print_error(summary: &str, error: &str) {
    let _ = Notification::new()
        .appname("REIKA Printer Service")
        .summary("Print Failed")
        .body(&format!("{}: {}", summary, error))
        .timeout(5000)
        .show();
}
```

#### Scenario: Success notification
- **WHEN** print job completes successfully
- **THEN** Windows toast shows "Print Completed" for 3 seconds

#### Scenario: Error notification
- **WHEN** print job fails
- **THEN** Windows toast shows "Print Failed" with error for 5 seconds

---

### Requirement: Request Body Schemas

The system SHALL validate request bodies against defined schemas.

**Implementation:** `src/models/request.rs`

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrinterTestSchema {
    test_page: bool,
    test_line: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Commands {
    pub commands: Vec<Command>,
}
```

#### Scenario: Test print request validation
- **WHEN** POST `/print/test` with `{"test_page": true, "test_line": "text"}`
- **THEN** deserializes to `PrinterTestSchema`

#### Scenario: Commands request validation
- **WHEN** POST `/print` with `{"commands": [...]}`
- **THEN** deserializes to `Commands` with vector of `Command` enums

---

## API Reference

### Endpoints

| Method | Path | Request Body | Success Response | Error Response |
|--------|------|--------------|------------------|----------------|
| GET | `/print/test` | None | `{"is_connected": true}` | `{"is_connected": false, "error": "..."}` |
| POST | `/print/test` | `PrinterTestSchema` | `{"is_connected": true}` | `{"is_connected": false, "error": "..."}` |
| POST | `/print` | `Commands` | `{"is_connected": true}` | `{"is_connected": false, "error": "..."}` |

### HTTP Status Codes

| Code | Meaning | When Used |
|------|---------|-----------|
| 200 | OK | Successful operation (including offline status check) |
| 400 | Bad Request | Invalid JSON, missing fields, unknown command |
| 500 | Internal Server Error | Printer error, USB failure (rare - retry usually succeeds) |

### CORS Headers

```
Access-Control-Allow-Origin: *
Access-Control-Allow-Methods: GET, POST, OPTIONS
Access-Control-Allow-Headers: Content-Type, Authorization, Accept, Origin
```

---

## Design Decisions

### Infallible Handler Pattern

Handlers return `Result<impl Reply, Infallible>`:

```rust
pub async fn handle_print(...) -> Result<impl Reply, Infallible>
```

This ensures:
1. All errors are converted to HTTP responses
2. No panics escape to client
3. Consistent error handling

### Print Log for GUI

Commands are stored with log entries:

```rust
log.add_success_with_commands(summary, commands_for_log);
```

This enables:
1. Receipt preview in GUI
2. Audit trail for troubleshooting
3. Persistent history across restarts

### Status Check Always Returns 200

Even when printer is offline, status endpoint returns 200:

```rust
Ok(warp::reply::with_status(json(&response), StatusCode::OK))
```

Rationale:
1. Request itself succeeded
2. `is_connected: false` indicates printer state
3. Simplifies client error handling
