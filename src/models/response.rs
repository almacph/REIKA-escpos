use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StatusResponse {
    pub is_connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl StatusResponse {
    pub fn success() -> Self {
        StatusResponse {
            is_connected: true,
            error: None,
        }
    }

    pub fn disconnected(error: impl Into<String>) -> Self {
        StatusResponse {
            is_connected: false,
            error: Some(error.into()),
        }
    }

    pub fn error(is_connected: bool, error: impl Into<String>) -> Self {
        StatusResponse {
            is_connected,
            error: Some(error.into()),
        }
    }
}
