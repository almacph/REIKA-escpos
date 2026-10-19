//! Read-only USB health contracts and deterministic seams.
//!
//! The worker and native driver use these contracts in the follow-up ownership
//! work. Keeping the state machine independent of rusb makes recovery rules
//! testable without a printer attached.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct GenerationId(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TransportState {
    Discovering,
    Available,
    Disconnected,
    Reconnecting,
    DriverError,
    Degraded,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Readiness {
    Ready,
    PaperOut,
    CoverOpen,
    PrinterError,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StatusCapability {
    Verified,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ErrorCategory {
    NoDevice,
    Access,
    Busy,
    Timeout,
    Pipe,
    InvalidInput,
    Other,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthError {
    pub category: ErrorCategory,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presence {
    Present,
    Absent,
    SoftError,
}

impl HealthError {
    pub fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum HealthReason {
    Startup,
    PresenceObserved,
    ProbeFailed,
    ReadinessUnknown,
    PaperOut,
    CoverOpen,
    PrinterFault,
    StaleGeneration,
    WriteSucceeded,
    WriteFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HealthSnapshot {
    pub transport: TransportState,
    pub readiness: Readiness,
    pub busy: bool,
    pub status_capability: StatusCapability,
    pub reason: HealthReason,
    pub generation: GenerationId,
    /// Monotonic milliseconds, used for freshness and timeout decisions.
    pub last_transport_millis: u64,
    pub last_readiness_millis: Option<u64>,
    pub last_successful_write_millis: Option<u64>,
    #[serde(default)]
    pub transport_age_millis: u64,
    #[serde(default)]
    pub readiness_age_millis: Option<u64>,
    /// Unix milliseconds, used only for operator-facing support output.
    pub observed_at_unix_millis: u128,
}

impl HealthSnapshot {
    pub fn initial(clock: &impl Clock) -> Self {
        Self {
            transport: TransportState::Discovering,
            readiness: Readiness::Unknown,
            busy: false,
            status_capability: StatusCapability::Unsupported,
            reason: HealthReason::Startup,
            generation: GenerationId(0),
            last_transport_millis: clock.monotonic_millis(),
            last_readiness_millis: None,
            last_successful_write_millis: None,
            transport_age_millis: 0,
            readiness_age_millis: None,
            observed_at_unix_millis: clock.wall_clock_millis(),
        }
    }

    pub fn transport_is_fresh(&self, now_millis: u64) -> bool {
        now_millis.saturating_sub(self.last_transport_millis) <= 15_000
    }

    pub fn readiness_is_fresh(&self, now_millis: u64) -> bool {
        self.last_readiness_millis
            .is_some_and(|seen| now_millis.saturating_sub(seen) <= 30_000)
    }

    /// Compatibility mapping retained for `/health` and `/print/test`.
    pub fn is_connected(&self, now_millis: u64) -> bool {
        self.transport == TransportState::Available
            && self.transport_is_fresh(now_millis)
            && !matches!(
                self.readiness,
                Readiness::PaperOut | Readiness::CoverOpen | Readiness::PrinterError
            )
    }

    /// Existing sensor vocabulary, with unsupported physical status remaining
    /// an ordinary online state rather than a USB error.
    pub fn sensor_value(&self, now_millis: u64) -> &'static str {
        if !self.is_connected(now_millis) {
            "OFFLINE"
        } else {
            "ONLINE"
        }
    }

    /// Return the externally visible state after applying freshness at read
    /// time. The worker may be blocked in native USB I/O, so cached state must
    /// not remain available indefinitely.
    pub fn refreshed(&self, clock: &impl Clock) -> Self {
        let mut snapshot = self.clone();
        let now = clock.monotonic_millis();
        snapshot.transport_age_millis = now.saturating_sub(snapshot.last_transport_millis);
        snapshot.readiness_age_millis = snapshot
            .last_readiness_millis
            .map(|seen| now.saturating_sub(seen));
        if snapshot.transport == TransportState::Available && !snapshot.transport_is_fresh(now) {
            snapshot.transport = TransportState::Degraded;
            snapshot.reason = HealthReason::ProbeFailed;
        }
        if snapshot.last_readiness_millis.is_some() && !snapshot.readiness_is_fresh(now) {
            snapshot.readiness = Readiness::Unknown;
            snapshot.reason = HealthReason::ReadinessUnknown;
        }
        snapshot
    }

    /// Record one successful native write. This is deliberately shared state
    /// so a long print refreshes transport evidence after every USB write.
    pub fn record_successful_write(&mut self, clock: &impl Clock) {
        let now = clock.monotonic_millis();
        self.last_successful_write_millis = Some(now);
        self.last_transport_millis = now;
        self.observed_at_unix_millis = clock.wall_clock_millis();
        self.reason = HealthReason::WriteSucceeded;
    }
}

pub trait Clock: Send + Sync {
    fn monotonic_millis(&self) -> u64;
    fn wall_clock_millis(&self) -> u128;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

static PROCESS_START: OnceLock<Instant> = OnceLock::new();

impl Clock for SystemClock {
    fn monotonic_millis(&self) -> u64 {
        PROCESS_START
            .get_or_init(Instant::now)
            .elapsed()
            .as_millis() as u64
    }

    fn wall_clock_millis(&self) -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    }
}

#[derive(Clone, Debug)]
pub struct FakeClock {
    monotonic: u64,
    wall: u128,
}

impl FakeClock {
    pub fn new() -> Self {
        Self {
            monotonic: 0,
            wall: 0,
        }
    }
    pub fn advance(&mut self, duration: Duration) {
        self.monotonic += duration.as_millis() as u64;
        self.wall += duration.as_millis();
    }

    pub fn jump_wall_millis(&mut self, delta: i128) {
        if delta.is_negative() {
            self.wall = self.wall.saturating_sub(delta.unsigned_abs());
        } else {
            self.wall = self.wall.saturating_add(delta as u128);
        }
    }
}

impl Clock for FakeClock {
    fn monotonic_millis(&self) -> u64 {
        self.monotonic
    }
    fn wall_clock_millis(&self) -> u128 {
        self.wall
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendOperation {
    Presence,
    StatusProbe,
    Write,
    Release,
    Open,
}

/// Minimal backend seam used by the worker and by regression tests.
pub trait UsbBackend {
    fn presence(&mut self) -> Result<Presence, HealthError>;
    fn status_probe(&mut self) -> Result<Readiness, HealthError>;
    fn write(&mut self, bytes: &[u8]) -> Result<(), HealthError>;
    fn release(&mut self) -> Result<(), HealthError>;
    fn open(&mut self) -> Result<(), HealthError>;
}

#[derive(Debug)]
pub struct FakeUsbBackend {
    pub operations: Vec<BackendOperation>,
    pub presence_result: Result<Presence, HealthError>,
    pub status_result: Result<Readiness, HealthError>,
    pub write_result: Result<(), HealthError>,
    pub open_result: Result<(), HealthError>,
    pub release_result: Result<(), HealthError>,
    pub writes: Vec<Vec<u8>>,
}

impl Default for FakeUsbBackend {
    fn default() -> Self {
        Self {
            operations: Vec::new(),
            presence_result: Ok(Presence::Absent),
            status_result: Err(HealthError::new(
                ErrorCategory::Timeout,
                "status unavailable",
            )),
            write_result: Err(HealthError::new(ErrorCategory::NoDevice, "no fake device")),
            open_result: Err(HealthError::new(ErrorCategory::NoDevice, "no fake device")),
            release_result: Ok(()),
            writes: Vec::new(),
        }
    }
}

impl FakeUsbBackend {
    pub fn available() -> Self {
        Self {
            presence_result: Ok(Presence::Present),
            status_result: Ok(Readiness::Unknown),
            write_result: Ok(()),
            open_result: Ok(()),
            release_result: Ok(()),
            ..Default::default()
        }
    }
}

impl UsbBackend for FakeUsbBackend {
    fn presence(&mut self) -> Result<Presence, HealthError> {
        self.operations.push(BackendOperation::Presence);
        self.presence_result.clone()
    }
    fn status_probe(&mut self) -> Result<Readiness, HealthError> {
        self.operations.push(BackendOperation::StatusProbe);
        self.status_result.clone()
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), HealthError> {
        self.operations.push(BackendOperation::Write);
        self.writes.push(bytes.to_vec());
        self.write_result.clone()
    }
    fn release(&mut self) -> Result<(), HealthError> {
        self.operations.push(BackendOperation::Release);
        self.release_result.clone()
    }
    fn open(&mut self) -> Result<(), HealthError> {
        self.operations.push(BackendOperation::Open);
        self.open_result.clone()
    }
}

/// The observation gate prevents a delayed old connection from overwriting a
/// newer connection's state.
#[derive(Clone, Debug)]
pub struct HealthTracker<C: Clock> {
    clock: C,
    snapshot: HealthSnapshot,
}

impl<C: Clock> HealthTracker<C> {
    pub fn new(clock: C) -> Self {
        Self {
            snapshot: HealthSnapshot::initial(&clock),
            clock,
        }
    }
    pub fn snapshot(&self) -> &HealthSnapshot {
        &self.snapshot
    }
    pub fn set_busy(&mut self, busy: bool) {
        self.snapshot.busy = busy;
    }
    pub fn generation(&self) -> GenerationId {
        self.snapshot.generation
    }
    pub fn next_generation(&mut self) -> GenerationId {
        self.snapshot.generation = GenerationId(self.snapshot.generation.0 + 1);
        self.snapshot.reason = HealthReason::Startup;
        self.snapshot.generation
    }
    pub fn observe_transport(
        &mut self,
        generation: GenerationId,
        available: bool,
        reason: HealthReason,
    ) -> bool {
        if generation < self.snapshot.generation {
            return false;
        }
        self.snapshot.generation = generation;
        self.snapshot.transport = if available {
            TransportState::Available
        } else {
            TransportState::Disconnected
        };
        self.snapshot.reason = reason;
        self.snapshot.last_transport_millis = self.clock.monotonic_millis();
        self.snapshot.observed_at_unix_millis = self.clock.wall_clock_millis();
        true
    }
    pub fn observe_readiness(
        &mut self,
        generation: GenerationId,
        readiness: Readiness,
        capability: StatusCapability,
    ) -> bool {
        if generation != self.snapshot.generation {
            return false;
        }
        self.snapshot.readiness = readiness;
        self.snapshot.status_capability = capability;
        self.snapshot.reason = match readiness {
            Readiness::Ready => HealthReason::PresenceObserved,
            Readiness::PaperOut => HealthReason::PaperOut,
            Readiness::CoverOpen => HealthReason::CoverOpen,
            Readiness::PrinterError => HealthReason::PrinterFault,
            Readiness::Unknown => HealthReason::ReadinessUnknown,
        };
        self.snapshot.last_readiness_millis = Some(self.clock.monotonic_millis());
        true
    }
    pub fn record_write(&mut self, generation: GenerationId) -> bool {
        if generation != self.snapshot.generation {
            return false;
        }
        self.snapshot.record_successful_write(&self.clock);
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointSet {
    pub interface: u8,
    pub output: u8,
    pub input: Option<u8>,
}

pub fn validate_endpoints(
    interface: u8,
    output: u8,
    input: Option<u8>,
) -> Result<EndpointSet, HealthError> {
    if output & 0x80 != 0 {
        return Err(HealthError::new(
            ErrorCategory::InvalidInput,
            "bulk output endpoint must be OUT",
        ));
    }
    if let Some(input) = input {
        if input & 0x80 == 0 {
            return Err(HealthError::new(
                ErrorCategory::InvalidInput,
                "bulk input endpoint must be IN",
            ));
        }
    }
    Ok(EndpointSet {
        interface,
        output,
        input,
    })
}

pub fn reconnect<B: UsbBackend>(backend: &mut B) -> Result<(), HealthError> {
    backend.release()?;
    backend.open()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_backend_reproduces_idle_recovery_without_init_probe() {
        let mut backend = FakeUsbBackend::available();
        assert_eq!(backend.presence(), Ok(Presence::Present));
        assert_eq!(backend.operations, vec![BackendOperation::Presence]);
    }

    #[test]
    fn fake_backend_exposes_interleaving_for_worker_regression() {
        let mut backend = FakeUsbBackend::available();
        backend.write(&[1, 2]).unwrap();
        backend.status_probe().unwrap();
        assert_eq!(
            backend.operations,
            vec![BackendOperation::Write, BackendOperation::StatusProbe]
        );
    }

    #[test]
    fn reconnect_releases_before_reopen() {
        let mut backend = FakeUsbBackend::available();
        reconnect(&mut backend).unwrap();
        assert_eq!(
            backend.operations,
            vec![BackendOperation::Release, BackendOperation::Open]
        );
    }

    #[test]
    fn asymmetric_and_output_only_endpoints_are_valid() {
        assert_eq!(
            validate_endpoints(1, 0x02, Some(0x83)).unwrap().input,
            Some(0x83)
        );
        assert_eq!(validate_endpoints(1, 0x02, None).unwrap().input, None);
        assert!(validate_endpoints(1, 0x82, None).is_err());
        assert!(validate_endpoints(1, 0x02, Some(0x03)).is_err());
    }

    #[test]
    fn stale_generation_cannot_reverse_newer_state() {
        let mut clock = FakeClock::new();
        let mut tracker = HealthTracker::new(clock.clone());
        assert!(tracker.observe_transport(GenerationId(1), true, HealthReason::PresenceObserved));
        tracker.observe_readiness(
            GenerationId(1),
            Readiness::Ready,
            StatusCapability::Verified,
        );
        clock.advance(Duration::from_secs(1));
        assert!(!tracker.observe_transport(GenerationId(0), false, HealthReason::ProbeFailed));
        assert_eq!(tracker.snapshot().transport, TransportState::Available);
    }

    #[test]
    fn unsupported_readiness_stays_compatible_and_freshness_is_bounded() {
        let mut clock = FakeClock::new();
        let mut tracker = HealthTracker::new(clock.clone());
        tracker.observe_transport(GenerationId(1), true, HealthReason::PresenceObserved);
        tracker.observe_readiness(
            GenerationId(1),
            Readiness::Unknown,
            StatusCapability::Unsupported,
        );
        assert!(tracker.snapshot().is_connected(clock.monotonic_millis()));
        assert_eq!(
            tracker.snapshot().sensor_value(clock.monotonic_millis()),
            "ONLINE"
        );
        clock.advance(Duration::from_secs(16));
        assert!(!tracker.snapshot().is_connected(clock.monotonic_millis()));
    }

    #[test]
    fn read_time_freshness_expires_while_owner_is_blocked() {
        let mut clock = FakeClock::new();
        let mut snapshot = HealthSnapshot::initial(&clock);
        snapshot.transport = TransportState::Available;
        snapshot.last_transport_millis = clock.monotonic_millis();
        snapshot.readiness = Readiness::Ready;
        snapshot.last_readiness_millis = Some(clock.monotonic_millis());

        clock.advance(Duration::from_secs(16));
        let stale = snapshot.refreshed(&clock);
        assert_eq!(stale.transport, TransportState::Degraded);
        assert_eq!(stale.readiness, Readiness::Ready);

        clock.advance(Duration::from_secs(15));
        assert_eq!(snapshot.refreshed(&clock).readiness, Readiness::Unknown);
    }

    #[test]
    fn each_successful_write_refreshes_transport_using_monotonic_time() {
        let mut clock = FakeClock::new();
        let mut snapshot = HealthSnapshot::initial(&clock);
        snapshot.transport = TransportState::Available;
        clock.advance(Duration::from_secs(16));
        assert_eq!(
            snapshot.refreshed(&clock).transport,
            TransportState::Degraded
        );

        snapshot.record_successful_write(&clock);
        clock.jump_wall_millis(86_400_000);
        assert_eq!(
            snapshot.refreshed(&clock).transport,
            TransportState::Available
        );
        assert_eq!(snapshot.last_successful_write_millis, Some(16_000));

        clock.advance(Duration::from_secs(16));
        assert_eq!(
            snapshot.refreshed(&clock).transport,
            TransportState::Degraded
        );
    }

    #[test]
    fn output_only_backend_does_not_claim_status_support() {
        let mut tracker = HealthTracker::new(FakeClock::new());
        tracker.observe_transport(GenerationId(1), true, HealthReason::PresenceObserved);
        assert!(!tracker.observe_readiness(
            GenerationId(0),
            Readiness::Ready,
            StatusCapability::Verified
        ));
        assert_eq!(
            tracker.snapshot().status_capability,
            StatusCapability::Unsupported
        );
    }
}
