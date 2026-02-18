use std::time::Duration;
use tokio::sync::{mpsc, watch};

/// Events sent by PrinterService and UsbDriver to trigger immediate sensor reports
#[derive(Debug, Clone)]
pub enum SensorEvent {
    UsbError(String),
    PrintFail(String),
}

/// Reports printer service health to the REIKA sensor dashboard.
///
/// Follows the same pattern as ESP8266 firmware (solenoid-http.ino):
/// - Periodic heartbeat with current state
/// - Immediate report on state changes
pub struct SensorReporter {
    client: reqwest::Client,
    api_key: String,
    server_url: String,
    current_state: String,
}

impl SensorReporter {
    pub fn new(api_key: String, server_url: String) -> Option<Self> {
        if api_key.is_empty() {
            return None;
        }

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(10))
            .build();

        match client {
            Ok(client) => {
                log::info!(
                    "SensorReporter initialized: server_url={}, api_key={}...",
                    server_url,
                    &api_key[..api_key.len().min(4)]
                );
                Some(Self {
                    client,
                    api_key,
                    server_url,
                    current_state: "OFFLINE".to_string(),
                })
            }
            Err(e) => {
                log::error!("SensorReporter: Failed to create HTTP client: {:?}", e);
                None
            }
        }
    }

    async fn report(&self, value: &str) {
        let url = format!("{}/api/sensors/report", self.server_url);
        log::debug!("SensorReporter: Reporting value={} to {}", value, url);

        let result = self
            .client
            .post(&url)
            .header("X-Sensor-Key", &self.api_key)
            .json(&serde_json::json!({ "value": value }))
            .send()
            .await;

        match result {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    log::debug!("SensorReporter: Report OK (status={})", status);
                } else {
                    log::warn!(
                        "SensorReporter: Report returned non-success status={} for value={}",
                        status,
                        value
                    );
                }
            }
            Err(e) => {
                log::warn!(
                    "SensorReporter: Failed to report value={}: {:?}",
                    value,
                    e
                );
            }
        }
    }

    /// Main loop: listens for online/offline status changes and sensor events.
    /// Sends heartbeat every 60 seconds with current state.
    pub async fn run(
        mut self,
        mut online_rx: watch::Receiver<bool>,
        mut event_rx: mpsc::Receiver<SensorEvent>,
    ) {
        log::info!("SensorReporter: Starting main loop");

        // Use interval instead of sleep so the timer is NOT reset when other
        // select! branches fire. sleep() inside select! gets dropped and
        // recreated each iteration, which caused heartbeats to never fire
        // when the watch channel was active.
        let mut heartbeat = tokio::time::interval(Duration::from_secs(60));
        heartbeat.tick().await; // consume the immediate first tick

        loop {
            tokio::select! {
                // Heartbeat timer - ticks every 60s regardless of other branches
                _ = heartbeat.tick() => {
                    self.report(&self.current_state.clone()).await;
                }

                // Online/offline status change
                result = online_rx.changed() => {
                    if result.is_err() {
                        log::warn!("SensorReporter: Online watch channel closed, stopping");
                        break;
                    }
                    let is_online = *online_rx.borrow();
                    let new_state = if is_online { "ONLINE" } else { "OFFLINE" };
                    if new_state != self.current_state {
                        log::info!(
                            "SensorReporter: State change {} -> {}",
                            self.current_state,
                            new_state
                        );
                        self.current_state = new_state.to_string();
                        self.report(&self.current_state.clone()).await;
                    }
                }

                // Critical error events (immediate report)
                Some(event) = event_rx.recv() => {
                    let (state, detail) = match &event {
                        SensorEvent::UsbError(msg) => ("USB_ERROR", msg.as_str()),
                        SensorEvent::PrintFail(msg) => ("PRINT_FAIL", msg.as_str()),
                    };
                    log::info!(
                        "SensorReporter: Critical event {} - {}",
                        state,
                        detail
                    );
                    self.current_state = state.to_string();
                    self.report(state).await;
                }
            }
        }

        log::info!("SensorReporter: Main loop ended");
    }
}
