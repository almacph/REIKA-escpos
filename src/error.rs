use std::fmt;

use escpos::errors::PrinterError;
use warp::http::StatusCode;

use crate::models::StatusResponse;

#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    InvalidInput(String),
    PrinterError(String),
    Internal(String),
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::InvalidInput(_) => StatusCode::BAD_REQUEST,
            AppError::PrinterError(_) | AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn to_response(&self, is_connected: bool) -> StatusResponse {
        StatusResponse::error(is_connected, self.to_string())
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            AppError::PrinterError(msg) => write!(f, "Printer error: {}", msg),
            AppError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl From<PrinterError> for AppError {
    fn from(e: PrinterError) -> Self {
        match e {
            PrinterError::Input(msg) => AppError::InvalidInput(msg),
            PrinterError::InvalidResponse(msg) => AppError::PrinterError(msg),
            PrinterError::Io(msg) => AppError::PrinterError(msg),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::InvalidInput(e.to_string())
    }
}
