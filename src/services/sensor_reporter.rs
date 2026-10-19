use std::time::Duration;
use tokio::sync::{mpsc, watch};

use super::health::{HealthSnapshot, SystemClock};

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
    report_sink: Option<mpsc::UnboundedSender<String>>,
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
                    report_sink: None,
                })
            }
            Err(e) => {
                log::error!("SensorReporter: Failed to create HTTP client: {:?}", e);
                None
            }
        }
    }

    #[cfg(test)]
    fn with_report_sink(mut self, sink: mpsc::UnboundedSender<String>) -> Self {
        self.report_sink = Some(sink);
        self
    }

    async fn report(&self, value: &str) {
        if let Some(sink) = &self.report_sink {
            let _ = sink.send(value.to_string());
            return;
        }
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
                log::warn!("SensorReporter: Failed to report value={}: {:?}", value, e);
            }
        }
    }

    /// Main loop: listens for online/offline status changes and sensor events.
    /// Sends heartbeat every 60 seconds with current state.
    pub async fn run(
        mut self,
        mut online_rx: watch::Receiver<bool>,
        mut health_rx: watch::Receiver<HealthSnapshot>,
        mut event_rx: mpsc::Receiver<SensorEvent>,
    ) {
        log::info!("SensorReporter: Starting main loop");

        self.current_state = if *online_rx.borrow() {
            "ONLINE".to_string()
        } else {
            "OFFLINE".to_string()
        };
        self.current_state =
            compatibility_state(&health_rx.borrow().refreshed(&SystemClock)).to_string();

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
                    let state = compatibility_state(&health_rx.borrow().refreshed(&SystemClock));
                    self.current_state = state.to_string();
                    self.report(state).await;
                }

                result = health_rx.changed() => {
                    if result.is_err() {
                        log::warn!("SensorReporter: Health watch closed, stopping");
                        break;
                    }
                    let state = compatibility_state(&health_rx.borrow().refreshed(&SystemClock));
                    if state != self.current_state {
                        self.current_state = state.to_string();
                        self.report(state).await;
                    }
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

fn compatibility_state(snapshot: &HealthSnapshot) -> &'static str {
    if snapshot.transport == super::health::TransportState::Available
        && snapshot.transport_age_millis <= 15_000
        && !matches!(
            snapshot.readiness,
            super::health::Readiness::PaperOut
                | super::health::Readiness::CoverOpen
                | super::health::Readiness::PrinterError
        )
    {
        "ONLINE"
    } else {
        "OFFLINE"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::health::{Clock, HealthReason, TransportState};
    use std::time::Duration;

    fn snapshot_online() -> HealthSnapshot {
        let clock = SystemClock;
        let mut snapshot = HealthSnapshot::initial(&clock);
        snapshot.transport = TransportState::Available;
        snapshot.reason = HealthReason::PresenceObserved;
        snapshot.last_transport_millis = clock.monotonic_millis();
        snapshot
    }

    #[tokio::test(start_paused = true)]
    async fn heartbeat_survives_repeated_state_events() {
        let (sink, mut reports) = mpsc::unbounded_channel();
        let reporter = SensorReporter::new("key".into(), "http://unused".into())
            .unwrap()
            .with_report_sink(sink);
        let (online_tx, online_rx) = watch::channel(true);
        let (_health_tx, health_rx) = watch::channel(snapshot_online());
        let (_event_tx, event_rx) = mpsc::channel(8);
        let task = tokio::spawn(reporter.run(online_rx, health_rx, event_rx));
        for _ in 0..10 {
            let _ = online_tx.send(false);
            let _ = online_tx.send(true);
        }
        tokio::time::advance(Duration::from_secs(61)).await;
        assert!(reports.recv().await.is_some());
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn fresh_health_observation_clears_critical_event_state() {
        let (sink, mut reports) = mpsc::unbounded_channel();
        let reporter = SensorReporter::new("key".into(), "http://unused".into())
            .unwrap()
            .with_report_sink(sink);
        let (_online_tx, online_rx) = watch::channel(true);
        let (health_tx, health_rx) = watch::channel(snapshot_online());
        let (event_tx, event_rx) = mpsc::channel(8);
        let task = tokio::spawn(reporter.run(online_rx, health_rx, event_rx));
        event_tx
            .send(SensorEvent::UsbError("lost".into()))
            .await
            .unwrap();
        assert_eq!(reports.recv().await.as_deref(), Some("USB_ERROR"));
        health_tx.send(snapshot_online()).unwrap();
        assert_eq!(reports.recv().await.as_deref(), Some("ONLINE"));
        task.abort();
    }
}
