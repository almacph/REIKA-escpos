#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use escpos::errors::PrinterError;
use escpos::printer::Printer;
use escpos::utils::Protocol;
use tokio::sync::watch;

use super::command_encoder::{encode_bit_image, encode_cash_drawer_pulse};
use super::health::{
    Clock, ErrorCategory, GenerationId, HealthError, HealthReason, HealthSnapshot, HealthTracker,
    Presence, Readiness, StatusCapability, SystemClock, TransportState,
};
use super::reprint::inject_reprint_markers;
use super::sensor_reporter::SensorEvent;
use super::usb_driver::{CustomUsbDriver, DeviceIdentity, UsbConfig};
use crate::app::file_logger::record_diagnostic_event;
use crate::error::AppError;
use crate::models::{Command, Commands, JustifyMode, PrinterTestSchema, UnderlineMode};

// Global counter for generating unique print IDs
static PRINT_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Generate a unique print ID for tracking operations
fn generate_print_id() -> String {
    let counter = PRINT_COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u32;
    format!("{:04x}{:04x}", timestamp & 0xFFFF, counter & 0xFFFF)
}

#[derive(Clone)]
pub struct PrinterService {
    worker: Arc<RwLock<Option<Arc<BlockingUsbWorker>>>>,
    health: Arc<RwLock<HealthSnapshot>>,
    publication: Arc<HealthPublication>,
    usb_config: UsbConfig,
    status_tx: Option<watch::Sender<bool>>,
    sensor_tx: Option<tokio::sync::mpsc::Sender<SensorEvent>>,
    transport: Option<Arc<dyn PrintTransport>>,
}

enum WorkerRequest {
    Print {
        commands: Commands,
        reprint: bool,
        reply: tokio::sync::oneshot::Sender<Result<(), AppError>>,
    },
    Test {
        request: PrinterTestSchema,
        reply: tokio::sync::oneshot::Sender<Result<(), AppError>>,
    },
    EnterMaintenance {
        reply: tokio::sync::oneshot::Sender<Result<(), AppError>>,
    },
}

struct BlockingUsbWorker {
    tx: mpsc::SyncSender<WorkerRequest>,
    health: Arc<RwLock<HealthSnapshot>>,
    stop: Arc<AtomicBool>,
    resume_maintenance: Arc<AtomicBool>,
    publication: Arc<HealthPublication>,
}

/// A lease proving that the blocking owner has released its native handle.
/// Dropping the lease requests discovery to resume; the request is also carried
/// by an atomic flag so a full print queue cannot strand maintenance forever.
pub struct MaintenanceGuard {
    resume: Arc<AtomicBool>,
    armed: bool,
}

impl Drop for MaintenanceGuard {
    fn drop(&mut self) {
        if self.armed {
            self.resume.store(true, Ordering::Release);
        }
    }
}

struct HealthPublication {
    status_tx: RwLock<Option<watch::Sender<bool>>>,
    health_tx: RwLock<Option<watch::Sender<HealthSnapshot>>>,
    sensor_tx: RwLock<Option<tokio::sync::mpsc::Sender<SensorEvent>>>,
}

impl HealthPublication {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            status_tx: RwLock::new(None),
            health_tx: RwLock::new(None),
            sensor_tx: RwLock::new(None),
        })
    }

    fn publish(&self, snapshot: &HealthSnapshot) {
        let connected = snapshot.transport == TransportState::Available
            && snapshot.transport_age_millis <= 15_000
            && !matches!(
                snapshot.readiness,
                Readiness::PaperOut | Readiness::CoverOpen | Readiness::PrinterError
            );
        if let Ok(slot) = self.status_tx.read() {
            if let Some(tx) = slot.as_ref() {
                let _ = tx.send(connected);
            }
        }
        if let Ok(slot) = self.health_tx.read() {
            if let Some(tx) = slot.as_ref() {
                let _ = tx.send(snapshot.clone());
            }
        }
    }

    fn send_sensor_event(&self, event: SensorEvent) {
        if let Ok(slot) = self.sensor_tx.read() {
            if let Some(tx) = slot.as_ref() {
                let _ = tx.try_send(event);
            }
        }
    }
}

fn retry_delay(base_seconds: u64, attempt: usize) -> Duration {
    let jitter = (base_seconds / 10).max(1).min(base_seconds);
    Duration::from_secs(base_seconds + (attempt as u64 % (jitter + 1)).min(jitter))
}

fn wait_retry(stop: &AtomicBool, duration: Duration) -> bool {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if stop.load(Ordering::Acquire) {
            return false;
        }
        std::thread::sleep(
            Duration::from_millis(25).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
    !stop.load(Ordering::Acquire)
}

trait OwnerBackend: Send {
    fn presence(&mut self) -> Result<Presence, HealthError>;
    fn print(
        &mut self,
        commands: Vec<Command>,
        reprint: bool,
        print_id: String,
    ) -> Result<(), PrinterError>;
    fn print_test(
        &mut self,
        request: PrinterTestSchema,
        print_id: String,
    ) -> Result<(), PrinterError>;
    fn release(&mut self) -> Result<(), HealthError>;
    fn status_capability(&self) -> StatusCapability {
        StatusCapability::Unsupported
    }
    fn status_probe(&mut self) -> Result<Readiness, HealthError> {
        Ok(Readiness::Unknown)
    }
}

type OwnerFactory = Arc<dyn Fn() -> Result<Box<dyn OwnerBackend>, HealthError> + Send + Sync>;

fn publish_successful_write<C: Clock>(
    health: &Arc<RwLock<HealthSnapshot>>,
    publication: &HealthPublication,
    clock: &C,
) {
    let snapshot = if let Ok(mut snapshot) = health.write() {
        snapshot.record_successful_write(clock);
        Some(snapshot.clone())
    } else {
        None
    };
    if let Some(snapshot) = snapshot {
        publication.publish(&snapshot.refreshed(clock));
    }
}

fn real_factory(
    config: UsbConfig,
    health: Arc<RwLock<HealthSnapshot>>,
    publication: Arc<HealthPublication>,
) -> OwnerFactory {
    let selected: Arc<std::sync::Mutex<Option<DeviceIdentity>>> =
        Arc::new(std::sync::Mutex::new(None));
    Arc::new(move || {
        let requested = selected.lock().ok().and_then(|identity| identity.clone());
        let driver = match requested.as_ref() {
            Some(identity) => CustomUsbDriver::open_selected(&config, Some(identity)),
            None => CustomUsbDriver::open(&config),
        }
        .map_err(|error| HealthError::new(ErrorCategory::Other, error.to_string()))?;
        let write_health = health.clone();
        let sensor_publication = publication.clone();
        let write_publication = publication.clone();
        let driver = driver
            .with_sensor_observer(Arc::new(move |event| {
                if let Ok(slot) = sensor_publication.sensor_tx.read() {
                    if let Some(tx) = slot.as_ref() {
                        let _ = tx.try_send(event);
                    }
                }
            }))
            .with_write_observer(Arc::new(move || {
                publish_successful_write(&write_health, &write_publication, &SystemClock);
            }));
        if let Ok(mut identity) = selected.lock() {
            *identity = Some(driver.identity().clone());
        }
        Ok(Box::new(ConfiguredOwnerBackend {
            config: config.clone(),
            identity: driver.identity().clone(),
            inner: RealOwnerBackend {
                driver: Some(driver),
            },
        }) as Box<dyn OwnerBackend>)
    })
}

struct RealOwnerBackend {
    driver: Option<CustomUsbDriver>,
}

impl OwnerBackend for RealOwnerBackend {
    fn presence(&mut self) -> Result<Presence, HealthError> {
        Err(HealthError::new(
            ErrorCategory::Other,
            "configured backend required",
        ))
    }

    fn print(
        &mut self,
        commands: Vec<Command>,
        reprint: bool,
        print_id: String,
    ) -> Result<(), PrinterError> {
        let driver = self
            .driver
            .as_ref()
            .cloned()
            .ok_or_else(|| PrinterError::Io("USB owner has no open driver".into()))?;
        if reprint {
            PrinterService::execute_reprint_inner(driver, commands, print_id)
        } else {
            PrinterService::execute_commands_inner(driver, commands, print_id)
        }
    }

    fn print_test(
        &mut self,
        request: PrinterTestSchema,
        print_id: String,
    ) -> Result<(), PrinterError> {
        let driver = self
            .driver
            .as_ref()
            .cloned()
            .ok_or_else(|| PrinterError::Io("USB owner has no open driver".into()))?;
        PrinterService::print_test_inner(driver, request, print_id)
    }

    fn release(&mut self) -> Result<(), HealthError> {
        // Dropping the handle releases the claimed interface. Keeping this
        // explicit in the owner lifecycle guarantees release precedes reopen.
        self.driver.take();
        Ok(())
    }
}

struct ConfiguredOwnerBackend {
    config: UsbConfig,
    identity: DeviceIdentity,
    inner: RealOwnerBackend,
}

impl OwnerBackend for ConfiguredOwnerBackend {
    fn presence(&mut self) -> Result<Presence, HealthError> {
        CustomUsbDriver::probe_presence_for_identity(&self.config, &self.identity)
    }
    fn print(
        &mut self,
        commands: Vec<Command>,
        reprint: bool,
        print_id: String,
    ) -> Result<(), PrinterError> {
        self.inner.print(commands, reprint, print_id)
    }
    fn print_test(
        &mut self,
        request: PrinterTestSchema,
        print_id: String,
    ) -> Result<(), PrinterError> {
        self.inner.print_test(request, print_id)
    }
    fn release(&mut self) -> Result<(), HealthError> {
        self.inner.release()
    }
    fn status_capability(&self) -> StatusCapability {
        StatusCapability::Unsupported
    }
}

impl BlockingUsbWorker {
    fn start(
        factory: OwnerFactory,
        health: Arc<RwLock<HealthSnapshot>>,
        publication: Arc<HealthPublication>,
    ) -> Arc<Self> {
        let (tx, rx) = mpsc::sync_channel(32);
        let thread_health = health.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let resume_maintenance = Arc::new(AtomicBool::new(false));
        let thread_resume_maintenance = resume_maintenance.clone();
        let thread_publication = publication.clone();
        std::thread::Builder::new()
            .name("reika-usb-owner".into())
            .spawn(move || {
                let mut owner: Option<Box<dyn OwnerBackend>> = factory().ok();
                let mut tracker = HealthTracker::new(SystemClock);
                if owner.is_some() {
                    tracker.observe_transport(
                        GenerationId(1),
                        true,
                        HealthReason::PresenceObserved,
                    );
                }
                if let Ok(mut snapshot) = thread_health.write() {
                    *snapshot = tracker.snapshot().clone();
                }
                if let Ok(snapshot) = thread_health.read() {
                    thread_publication.publish(&snapshot.refreshed(&SystemClock));
                }
                let mut next_probe = std::time::Instant::now();
                let mut backoff = 0usize;
                let mut soft_probe_failures = 0u8;
                let mut next_status = std::time::Instant::now() + Duration::from_secs(15);
                let mut status_bad = 0u8;
                let mut status_disabled = false;
                let mut maintenance = false;
                let delays = [1, 2, 5, 10, 30];
                while !thread_stop.load(Ordering::Acquire) {
                    if maintenance && thread_resume_maintenance.swap(false, Ordering::AcqRel) {
                        maintenance = false;
                        next_probe = std::time::Instant::now();
                        next_status = std::time::Instant::now() + Duration::from_secs(15);
                        let generation = tracker.next_generation();
                        tracker.observe_transport(generation, false, HealthReason::Startup);
                        if let Ok(mut snapshot) = thread_health.write() {
                            *snapshot = tracker.snapshot().clone();
                        }
                    }
                    if let Ok(snapshot) = thread_health.read() {
                        thread_publication.publish(&snapshot.refreshed(&SystemClock));
                    }
                    let timeout = if maintenance {
                        Duration::from_millis(50)
                    } else {
                        next_probe.saturating_duration_since(std::time::Instant::now())
                    };
                    match rx.recv_timeout(timeout) {
                        Ok(WorkerRequest::Print {
                            commands,
                            reprint,
                            reply,
                        }) => {
                            if owner.is_some() {
                                tracker.set_busy(true);
                                if let Ok(mut snapshot) = thread_health.write() {
                                    *snapshot = tracker.snapshot().clone();
                                }
                                if let Ok(snapshot) = thread_health.read() {
                                    thread_publication.publish(&snapshot.refreshed(&SystemClock));
                                }
                                let print_id = generate_print_id();
                                let command_vec = if reprint {
                                    super::reprint::inject_reprint_markers(commands.commands)
                                } else {
                                    commands.commands
                                };
                                let result = loop {
                                    let Some(mut current) = owner.take() else {
                                        if !wait_retry(
                                            &thread_stop,
                                            retry_delay(
                                                delays[backoff.min(delays.len() - 1)],
                                                backoff,
                                            ),
                                        ) {
                                            break Err(PrinterError::Io(
                                                "printer worker stopped".into(),
                                            ));
                                        }
                                        match factory() {
                                            Ok(reopened) => {
                                                owner = Some(reopened);
                                                backoff = 0;
                                                status_bad = 0;
                                                status_disabled = false;
                                                next_status = std::time::Instant::now()
                                                    + Duration::from_secs(15);
                                                let generation = tracker.next_generation();
                                                tracker.observe_transport(
                                                    generation,
                                                    true,
                                                    HealthReason::PresenceObserved,
                                                );
                                                if let Ok(mut snapshot) = thread_health.write() {
                                                    *snapshot = tracker.snapshot().clone();
                                                }
                                            }
                                            Err(_) => backoff = (backoff + 1).min(delays.len() - 1),
                                        }
                                        continue;
                                    };
                                    let attempt = current.print(
                                        command_vec.clone(),
                                        reprint,
                                        print_id.clone(),
                                    );
                                    match attempt {
                                        Ok(()) => {
                                            owner = Some(current);
                                            tracker.record_write(tracker.generation());
                                            if let Ok(mut snapshot) = thread_health.write() {
                                                *snapshot = tracker.snapshot().clone();
                                            }
                                            break Ok(());
                                        }
                                        Err(PrinterError::Input(message)) => {
                                            owner = Some(current);
                                            break Err(PrinterError::Input(message));
                                        }
                                        Err(error) => {
                                            thread_publication.send_sensor_event(
                                                SensorEvent::PrintFail(error.to_string()),
                                            );
                                            let _ = current.release();
                                            if let Ok(mut snapshot) = thread_health.write() {
                                                snapshot.transport = TransportState::Disconnected;
                                                snapshot.reason = HealthReason::WriteFailed;
                                            }
                                            // A printer error can be raised after the driver has
                                            // already emitted part of the command stream. Reopening
                                            // the device and replaying `command_vec` would duplicate
                                            // the receipt. Leave recovery to the next explicit print
                                            // request after the transport is rediscovered.
                                            break Err(error);
                                        }
                                    }
                                };
                                tracker.set_busy(false);
                                if let Ok(mut snapshot) = thread_health.write() {
                                    *snapshot = tracker.snapshot().clone();
                                }
                                if let Ok(snapshot) = thread_health.read() {
                                    thread_publication.publish(&snapshot.refreshed(&SystemClock));
                                }
                                let _ = reply.send(result.map_err(AppError::from));
                            } else {
                                let _ = reply.send(Err(AppError::Internal(
                                    "printer transport unavailable".into(),
                                )));
                            }
                        }
                        Ok(WorkerRequest::Test { request, reply }) => {
                            tracker.set_busy(true);
                            if let Ok(mut snapshot) = thread_health.write() {
                                *snapshot = tracker.snapshot().clone();
                            }
                            if let Ok(snapshot) = thread_health.read() {
                                thread_publication.publish(&snapshot.refreshed(&SystemClock));
                            }
                            let result = owner
                                .as_mut()
                                .map(|current| {
                                    current
                                        .print_test(request, generate_print_id())
                                        .map_err(AppError::from)
                                })
                                .unwrap_or_else(|| {
                                    Err(AppError::Internal("printer transport unavailable".into()))
                                });
                            tracker.set_busy(false);
                            if let Ok(mut snapshot) = thread_health.write() {
                                *snapshot = tracker.snapshot().clone();
                            }
                            if let Ok(snapshot) = thread_health.read() {
                                thread_publication.publish(&snapshot.refreshed(&SystemClock));
                            }
                            let _ = reply.send(result);
                        }
                        Ok(WorkerRequest::EnterMaintenance { reply }) => {
                            if maintenance || tracker.snapshot().busy {
                                let _ = reply.send(Err(AppError::Busy(
                                    "printer is busy; maintenance deferred".into(),
                                )));
                                continue;
                            }
                            // Take and release the only backend before acknowledging. The
                            // handle is dropped with the backend before the reply is sent.
                            let release_result = owner.as_mut().map(|current| current.release());
                            owner = None;
                            if let Some(Err(error)) = release_result {
                                maintenance = false;
                                next_probe = std::time::Instant::now();
                                let _ = reply.send(Err(AppError::Internal(format!(
                                    "failed to release printer for maintenance: {}",
                                    error.message
                                ))));
                                continue;
                            }
                            maintenance = true;
                            tracker.observe_transport(
                                tracker.generation(),
                                false,
                                HealthReason::ProbeFailed,
                            );
                            if let Ok(mut snapshot) = thread_health.write() {
                                *snapshot = tracker.snapshot().clone();
                            }
                            if let Ok(snapshot) = thread_health.read() {
                                thread_publication.publish(&snapshot.refreshed(&SystemClock));
                            }
                            // If the caller cancelled while the native release was in
                            // progress, do not strand the owner in maintenance mode.
                            if reply.send(Ok(())).is_err() {
                                maintenance = false;
                                next_probe = std::time::Instant::now();
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if maintenance {
                                continue;
                            }
                            next_probe = std::time::Instant::now() + Duration::from_secs(5);
                            if let Some(mut current) = owner.take() {
                                if !status_disabled
                                    && std::time::Instant::now() >= next_status
                                    && current.status_capability() == StatusCapability::Verified
                                {
                                    match current.status_probe() {
                                        Ok(readiness) => {
                                            status_bad = 0;
                                            tracker.observe_readiness(
                                                tracker.generation(),
                                                readiness,
                                                StatusCapability::Verified,
                                            );
                                            if let Ok(mut snapshot) = thread_health.write() {
                                                *snapshot = tracker.snapshot().clone();
                                            }
                                        }
                                        Err(_) => {
                                            status_bad = status_bad.saturating_add(1);
                                            tracker.observe_readiness(
                                                tracker.generation(),
                                                Readiness::Unknown,
                                                if status_bad >= 3 {
                                                    StatusCapability::Unsupported
                                                } else {
                                                    StatusCapability::Verified
                                                },
                                            );
                                            if status_bad >= 3 {
                                                status_disabled = true;
                                            }
                                            if let Ok(mut snapshot) = thread_health.write() {
                                                *snapshot = tracker.snapshot().clone();
                                            }
                                        }
                                    }
                                    next_status =
                                        std::time::Instant::now() + Duration::from_secs(15);
                                }
                                match current.presence() {
                                    Ok(Presence::Present) => {
                                        soft_probe_failures = 0;
                                        owner = Some(current);
                                        let generation = tracker.generation();
                                        tracker.observe_transport(
                                            generation,
                                            true,
                                            HealthReason::PresenceObserved,
                                        );
                                        if let Ok(mut snapshot) = thread_health.write() {
                                            *snapshot = tracker.snapshot().clone();
                                        }
                                    }
                                    Ok(Presence::Absent) => {
                                        let _ = current.release();
                                        backoff = 0;
                                        let generation = tracker.next_generation();
                                        tracker.observe_transport(
                                            generation,
                                            false,
                                            HealthReason::ProbeFailed,
                                        );
                                        if let Ok(mut snapshot) = thread_health.write() {
                                            *snapshot = tracker.snapshot().clone();
                                        }
                                    }
                                    Ok(Presence::SoftError) | Err(_) => {
                                        soft_probe_failures = soft_probe_failures.saturating_add(1);
                                        if soft_probe_failures >= 2 {
                                            let _ = current.release();
                                            let generation = tracker.next_generation();
                                            tracker.observe_transport(
                                                generation,
                                                false,
                                                HealthReason::ProbeFailed,
                                            );
                                            if let Ok(mut snapshot) = thread_health.write() {
                                                *snapshot = tracker.snapshot().clone();
                                            }
                                        } else {
                                            owner = Some(current);
                                        }
                                    }
                                }
                            }
                            if owner.is_none() {
                                match factory() {
                                    Ok(reopened) => {
                                        owner = Some(reopened);
                                        backoff = 0;
                                        status_bad = 0;
                                        status_disabled = false;
                                        next_status =
                                            std::time::Instant::now() + Duration::from_secs(15);
                                        let generation = tracker.next_generation();
                                        tracker.observe_transport(
                                            generation,
                                            true,
                                            HealthReason::PresenceObserved,
                                        );
                                        if let Ok(mut snapshot) = thread_health.write() {
                                            *snapshot = tracker.snapshot().clone();
                                        }
                                    }
                                    Err(_) => {
                                        backoff = (backoff + 1).min(delays.len() - 1);
                                        next_probe = std::time::Instant::now()
                                            + retry_delay(delays[backoff], backoff);
                                    }
                                }
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("USB owner thread must start");
        Arc::new(Self {
            tx,
            health,
            stop,
            resume_maintenance,
            publication,
        })
    }

    fn submit(&self, request: WorkerRequest) -> Result<(), AppError> {
        self.tx.try_send(request).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => {
                AppError::Busy("printer queue is full; retry later".into())
            }
            mpsc::TrySendError::Disconnected(_) => {
                AppError::Internal("printer worker stopped".into())
            }
        })
    }
}

impl Drop for BlockingUsbWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.resume_maintenance.store(true, Ordering::Release);
    }
}

pub trait PrintTransport: Send + Sync {
    fn execute(&self, job: &PreparedPrint) -> Result<(), AppError>;
}

pub struct PreparedPrint {
    pub commands: Vec<Command>,
    pub special_bytes: Vec<Vec<u8>>,
    pub reprint: bool,
}

fn prepare_print(commands: &Commands, reprint: bool) -> Result<PreparedPrint, AppError> {
    commands.validate().map_err(AppError::InvalidInput)?;
    let commands = if reprint {
        inject_reprint_markers(commands.commands.clone())
    } else {
        commands.commands.clone()
    };
    let special_bytes = commands
        .iter()
        .filter_map(|command| match command {
            Command::CashDrawerPulse(pulse) => Some(encode_cash_drawer_pulse(pulse)),
            Command::BitImage(image) => encode_bit_image(image).ok(),
            _ => None,
        })
        .collect();
    Ok(PreparedPrint {
        commands,
        special_bytes,
        reprint,
    })
}

impl PrinterService {
    pub fn new(driver: CustomUsbDriver, usb_config: UsbConfig) -> Self {
        let _ = driver;
        let health = Arc::new(RwLock::new(HealthSnapshot::initial(&SystemClock)));
        let publication = HealthPublication::new();
        Self {
            worker: Arc::new(RwLock::new(Some(BlockingUsbWorker::start(
                real_factory(usb_config.clone(), health.clone(), publication.clone()),
                health.clone(),
                publication.clone(),
            )))),
            health,
            publication,
            usb_config,
            status_tx: None,
            sensor_tx: None,
            transport: None,
        }
    }

    pub fn unavailable(usb_config: UsbConfig) -> Self {
        let health = Arc::new(RwLock::new(HealthSnapshot::initial(&SystemClock)));
        let publication = HealthPublication::new();
        let worker = BlockingUsbWorker::start(
            real_factory(usb_config.clone(), health.clone(), publication.clone()),
            health.clone(),
            publication.clone(),
        );
        Self {
            worker: Arc::new(RwLock::new(Some(worker))),
            health,
            publication,
            usb_config,
            status_tx: None,
            sensor_tx: None,
            transport: None,
        }
    }

    pub fn attach_driver(&self, driver: CustomUsbDriver) {
        let _ = driver;
        let worker = BlockingUsbWorker::start(
            real_factory(
                self.usb_config.clone(),
                self.health.clone(),
                self.publication.clone(),
            ),
            self.health.clone(),
            self.publication.clone(),
        );
        if let Ok(mut slot) = self.worker.write() {
            *slot = Some(worker);
        }
    }

    pub fn with_transport(mut self, transport: Arc<dyn PrintTransport>) -> Self {
        self.transport = Some(transport);
        self
    }

    #[cfg(test)]
    pub fn test_service(transport: Arc<dyn PrintTransport>) -> Self {
        Self {
            worker: Arc::new(RwLock::new(None)),
            health: Arc::new(RwLock::new(HealthSnapshot::initial(&SystemClock))),
            publication: HealthPublication::new(),
            usb_config: UsbConfig {
                vendor_id: 0,
                product_id: 0,
                endpoint: None,
                interface: None,
            },
            status_tx: None,
            sensor_tx: None,
            transport: Some(transport),
        }
    }

    pub fn with_status(mut self, status_tx: watch::Sender<bool>) -> Self {
        if let Ok(mut slot) = self.publication.status_tx.write() {
            *slot = Some(status_tx.clone());
        }
        self.status_tx = Some(status_tx);
        self
    }

    pub fn with_health_watch(self, health_tx: watch::Sender<HealthSnapshot>) -> Self {
        if let Ok(mut slot) = self.publication.health_tx.write() {
            *slot = Some(health_tx);
        }
        self
    }

    pub fn with_sensor(mut self, sensor_tx: tokio::sync::mpsc::Sender<SensorEvent>) -> Self {
        if let Ok(mut slot) = self.publication.sensor_tx.write() {
            *slot = Some(sensor_tx.clone());
        }
        self.sensor_tx = Some(sensor_tx);
        self
    }

    fn send_sensor_event(&self, event: SensorEvent) {
        if let Some(tx) = &self.sensor_tx {
            let _ = tx.try_send(event);
        }
    }

    fn update_status(&self, online: bool) {
        record_diagnostic_event(
            if online {
                "printer_online"
            } else {
                "printer_offline"
            },
            "printer_state",
            None,
        );
        if let Some(tx) = &self.status_tx {
            let _ = tx.send(online);
        }
    }

    pub fn try_open(config: &UsbConfig) -> Option<CustomUsbDriver> {
        log::debug!(
            "try_open: VID=0x{:04X}, PID=0x{:04X}, EP={:?}, IF={:?}",
            config.vendor_id,
            config.product_id,
            config.endpoint,
            config.interface
        );
        match CustomUsbDriver::open(config) {
            Ok(driver) => {
                log::info!("try_open: USB device opened successfully");
                Some(driver)
            }
            Err(e) => {
                log::debug!("try_open: USB open failed: {:?}", e);
                None
            }
        }
    }

    pub async fn check_connection(&self) -> bool {
        self.health
            .read()
            .map(|snapshot| {
                let snapshot = snapshot.refreshed(&SystemClock);
                snapshot.transport == TransportState::Available
                    && !matches!(
                        snapshot.readiness,
                        super::health::Readiness::PaperOut
                            | super::health::Readiness::CoverOpen
                            | super::health::Readiness::PrinterError
                    )
            })
            .unwrap_or(false)
    }

    pub fn health_snapshot(&self) -> HealthSnapshot {
        self.health
            .read()
            .map(|snapshot| snapshot.refreshed(&SystemClock))
            .unwrap_or_else(|_| HealthSnapshot::initial(&SystemClock))
    }

    /// Release the native USB handle and hold discovery while a maintenance
    /// operation owns the device. Dropping the returned lease resumes the
    /// blocking owner and starts a fresh discovery generation.
    pub async fn enter_maintenance(&self) -> Result<MaintenanceGuard, AppError> {
        let worker = self
            .worker
            .read()
            .ok()
            .and_then(|slot| slot.clone())
            .ok_or_else(|| AppError::Internal("printer transport unavailable".into()))?;
        if self.health_snapshot().busy {
            return Err(AppError::Busy(
                "printer is busy; maintenance deferred".into(),
            ));
        }
        let mut guard = MaintenanceGuard {
            resume: worker.resume_maintenance.clone(),
            armed: false,
        };
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        worker.submit(WorkerRequest::EnterMaintenance { reply: reply_tx })?;
        tokio::time::timeout(Duration::from_secs(1), reply_rx)
            .await
            .map_err(|_| AppError::Busy("maintenance acknowledgement timed out".into()))?
            .map_err(|_| AppError::Internal("printer worker stopped".into()))??;
        guard.armed = true;
        Ok(guard)
    }

    pub async fn execute_commands(&self, commands: Commands) -> Result<(), AppError> {
        let prepared = prepare_print(&commands, false)?;
        if let Some(transport) = &self.transport {
            return transport.execute(&prepared);
        }
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.worker
            .read()
            .ok()
            .and_then(|slot| slot.clone())
            .ok_or_else(|| AppError::Internal("printer transport unavailable".into()))?
            .submit(WorkerRequest::Print {
                commands,
                reprint: false,
                reply: reply_tx,
            })?;
        reply_rx
            .await
            .map_err(|_| AppError::Internal("printer worker stopped".into()))?
    }

    /// Execute a reprint: injects anti-fraud markers at top, middle, and bottom,
    /// then prints with retry. Does NOT log to print log (not a new transaction).
    pub async fn execute_reprint_commands(&self, commands: Commands) -> Result<(), AppError> {
        let prepared = prepare_print(&commands, true)?;
        if let Some(transport) = &self.transport {
            return transport.execute(&prepared);
        }
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.worker
            .read()
            .ok()
            .and_then(|slot| slot.clone())
            .ok_or_else(|| AppError::Internal("printer transport unavailable".into()))?
            .submit(WorkerRequest::Print {
                commands,
                reprint: true,
                reply: reply_tx,
            })?;
        reply_rx
            .await
            .map_err(|_| AppError::Internal("printer worker stopped".into()))?
    }

    pub(crate) fn execute_reprint_inner(
        driver: CustomUsbDriver,
        commands: Vec<Command>,
        print_id: String,
    ) -> Result<(), PrinterError> {
        let start = Instant::now();
        let cmd_count = commands.len();
        log::info!(
            "[print_id={}] execute_reprint: Starting {} commands (with markers)...",
            print_id,
            cmd_count
        );

        let mut printer = Printer::new(driver, Protocol::default(), None);

        // The injected command stream already starts with Init, so we iterate directly
        for (idx, command) in commands.iter().enumerate() {
            let cmd_start = Instant::now();
            let result = match command {
                Command::Print(_) => printer.print(),
                Command::Init(_) => printer.init(),
                Command::Reset(_) => printer.reset(),
                Command::Cut(_) => printer.cut(),
                Command::PartialCut(_) => printer.partial_cut(),
                Command::PrintCut(_) => printer.print_cut(),
                Command::PageCode(page_code) => printer.page_code(page_code.clone().into()),
                Command::CharacterSet(char_set) => printer.character_set(char_set.clone().into()),
                Command::Bold(enabled) => printer.bold(*enabled),
                Command::Italic(_) => Ok(&mut printer),
                Command::Underline(mode) => printer.underline(mode.clone().into()),
                Command::DoubleStrike(enabled) => printer.double_strike(*enabled),
                Command::Font(font) => printer.font(font.clone().into()),
                Command::Flip(enabled) => printer.flip(*enabled),
                Command::Justify(mode) => printer.justify(mode.clone().into()),
                Command::Reverse(enabled) => printer.reverse(*enabled),
                Command::Size((width, height)) => printer.size(*width, *height),
                Command::ResetSize(_) => printer.reset_size(),
                Command::Smoothing(enabled) => printer.smoothing(*enabled),
                Command::Feed(_) => printer.feed(),
                Command::Feeds(lines) => printer.feeds(*lines),
                Command::LineSpacing(value) => printer.line_spacing(*value),
                Command::ResetLineSpacing(_) => printer.reset_line_spacing(),
                Command::UpsideDown(enabled) => printer.upside_down(*enabled),
                Command::CashDrawer(pin) => printer.cash_drawer(pin.clone().into()),
                Command::CashDrawerPulse(pulse) => printer.custom(&encode_cash_drawer_pulse(pulse)),
                Command::BitImage(image) => encode_bit_image(image)
                    .map_err(PrinterError::Input)
                    .and_then(|bytes| printer.custom(&bytes)),
                Command::Write(text) => printer.write(text),
                Command::Writeln(text) => printer.writeln(text),
                Command::Ean13(data) => printer.ean13(data),
                Command::Ean8(data) => printer.ean8(data),
                Command::Upca(data) => printer.upca(data),
                Command::Upce(data) => printer.upce(data),
                Command::Code39(data) => printer.code39(data),
                Command::Codabar(data) => printer.codabar(data),
                Command::Itf(data) => printer.itf(data),
                Command::Qrcode(data) => printer.qrcode(data),
                Command::GS1Databar2d(data) => printer.gs1_databar_2d(data),
                Command::Pdf417(data) => printer.pdf417(data),
                Command::MaxiCode(data) => printer.maxi_code(data),
                Command::DataMatrix(data) => printer.data_matrix(data),
                Command::Aztec(data) => printer.aztec(data),
            };

            match result {
                Ok(_) => {
                    log::debug!(
                        "execute_reprint: [{}/{}] OK in {:?}",
                        idx + 1,
                        cmd_count,
                        cmd_start.elapsed()
                    );
                }
                Err(e) => {
                    log::error!(
                        "execute_reprint: [{}/{}] FAILED after {:?}: {:?}",
                        idx + 1,
                        cmd_count,
                        cmd_start.elapsed(),
                        e
                    );
                    return Err(e);
                }
            }
        }

        log::info!(
            "[print_id={}] execute_reprint: COMPLETE - {} commands in {:?}",
            print_id,
            cmd_count,
            start.elapsed()
        );
        Ok(())
    }

    pub(crate) fn execute_commands_inner(
        driver: CustomUsbDriver,
        commands: Vec<Command>,
        print_id: String,
    ) -> Result<(), PrinterError> {
        let start = Instant::now();
        let cmd_count = commands.len();
        log::info!(
            "[print_id={}] execute_commands: Starting {} commands...",
            print_id,
            cmd_count
        );

        let mut printer = Printer::new(driver, Protocol::default(), None);

        log::debug!("execute_commands: Sending init...");
        printer.init()?;
        log::debug!("execute_commands: Init OK");

        for (idx, command) in commands.iter().enumerate() {
            let cmd_start = Instant::now();
            let cmd_name = match command {
                Command::Print(_) => "Print",
                Command::Init(_) => "Init",
                Command::Reset(_) => "Reset",
                Command::Cut(_) => "Cut",
                Command::PartialCut(_) => "PartialCut",
                Command::PrintCut(_) => "PrintCut",
                Command::PageCode(_) => "PageCode",
                Command::CharacterSet(_) => "CharacterSet",
                Command::Bold(_) => "Bold",
                Command::Italic(_) => "Italic",
                Command::Underline(_) => "Underline",
                Command::DoubleStrike(_) => "DoubleStrike",
                Command::Font(_) => "Font",
                Command::Flip(_) => "Flip",
                Command::Justify(_) => "Justify",
                Command::Reverse(_) => "Reverse",
                Command::Size(_) => "Size",
                Command::ResetSize(_) => "ResetSize",
                Command::Smoothing(_) => "Smoothing",
                Command::Feed(_) => "Feed",
                Command::Feeds(_) => "Feeds",
                Command::LineSpacing(_) => "LineSpacing",
                Command::ResetLineSpacing(_) => "ResetLineSpacing",
                Command::UpsideDown(_) => "UpsideDown",
                Command::CashDrawer(_) => "CashDrawer",
                Command::CashDrawerPulse(_) => "CashDrawerPulse",
                Command::BitImage(_) => "BitImage",
                Command::Write(_) => "Write",
                Command::Writeln(_) => "Writeln",
                Command::Ean13(_) => "Ean13",
                Command::Ean8(_) => "Ean8",
                Command::Upca(_) => "Upca",
                Command::Upce(_) => "Upce",
                Command::Code39(_) => "Code39",
                Command::Codabar(_) => "Codabar",
                Command::Itf(_) => "Itf",
                Command::Qrcode(_) => "Qrcode",
                Command::GS1Databar2d(_) => "GS1Databar2d",
                Command::Pdf417(_) => "Pdf417",
                Command::MaxiCode(_) => "MaxiCode",
                Command::DataMatrix(_) => "DataMatrix",
                Command::Aztec(_) => "Aztec",
            };

            let result = match command {
                Command::Print(_) => printer.print(),
                Command::Init(_) => printer.init(),
                Command::Reset(_) => printer.reset(),
                Command::Cut(_) => printer.cut(),
                Command::PartialCut(_) => printer.partial_cut(),
                Command::PrintCut(_) => printer.print_cut(),
                Command::PageCode(page_code) => printer.page_code(page_code.clone().into()),
                Command::CharacterSet(char_set) => printer.character_set(char_set.clone().into()),
                Command::Bold(enabled) => printer.bold(*enabled),
                Command::Italic(_) => Ok(&mut printer),
                Command::Underline(mode) => printer.underline(mode.clone().into()),
                Command::DoubleStrike(enabled) => printer.double_strike(*enabled),
                Command::Font(font) => printer.font(font.clone().into()),
                Command::Flip(enabled) => printer.flip(*enabled),
                Command::Justify(mode) => printer.justify(mode.clone().into()),
                Command::Reverse(enabled) => printer.reverse(*enabled),
                Command::Size((width, height)) => printer.size(*width, *height),
                Command::ResetSize(_) => printer.reset_size(),
                Command::Smoothing(enabled) => printer.smoothing(*enabled),
                Command::Feed(_) => printer.feed(),
                Command::Feeds(lines) => printer.feeds(*lines),
                Command::LineSpacing(value) => printer.line_spacing(*value),
                Command::ResetLineSpacing(_) => printer.reset_line_spacing(),
                Command::UpsideDown(enabled) => printer.upside_down(*enabled),
                Command::CashDrawer(pin) => printer.cash_drawer(pin.clone().into()),
                Command::CashDrawerPulse(pulse) => printer.custom(&encode_cash_drawer_pulse(pulse)),
                Command::BitImage(image) => encode_bit_image(image)
                    .map_err(PrinterError::Input)
                    .and_then(|bytes| printer.custom(&bytes)),
                Command::Write(text) => printer.write(text),
                Command::Writeln(text) => printer.writeln(text),
                Command::Ean13(data) => printer.ean13(data),
                Command::Ean8(data) => printer.ean8(data),
                Command::Upca(data) => printer.upca(data),
                Command::Upce(data) => printer.upce(data),
                Command::Code39(data) => printer.code39(data),
                Command::Codabar(data) => printer.codabar(data),
                Command::Itf(data) => printer.itf(data),
                Command::Qrcode(data) => printer.qrcode(data),
                Command::GS1Databar2d(data) => printer.gs1_databar_2d(data),
                Command::Pdf417(data) => printer.pdf417(data),
                Command::MaxiCode(data) => printer.maxi_code(data),
                Command::DataMatrix(data) => printer.data_matrix(data),
                Command::Aztec(data) => printer.aztec(data),
            };

            match result {
                Ok(_) => {
                    log::debug!(
                        "execute_commands: [{}/{}] {} OK in {:?}",
                        idx + 1,
                        cmd_count,
                        cmd_name,
                        cmd_start.elapsed()
                    );
                }
                Err(e) => {
                    log::error!(
                        "execute_commands: [{}/{}] {} FAILED after {:?}: {:?}",
                        idx + 1,
                        cmd_count,
                        cmd_name,
                        cmd_start.elapsed(),
                        e
                    );
                    return Err(e);
                }
            }
        }

        log::debug!(
            "[print_id={}] execute_commands: Sending final print_cut...",
            print_id
        );
        let cut_start = Instant::now();
        printer.print_cut()?;
        log::debug!(
            "[print_id={}] execute_commands: print_cut OK in {:?}",
            print_id,
            cut_start.elapsed()
        );

        log::info!(
            "[print_id={}] execute_commands: COMPLETE - {} commands in {:?}",
            print_id,
            cmd_count,
            start.elapsed()
        );
        Ok(())
    }

    pub async fn print_test(&self, request: PrinterTestSchema) -> Result<(), AppError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.worker
            .read()
            .ok()
            .and_then(|slot| slot.clone())
            .ok_or_else(|| AppError::Internal("printer transport unavailable".into()))?
            .submit(WorkerRequest::Test {
                request,
                reply: reply_tx,
            })?;
        reply_rx
            .await
            .map_err(|_| AppError::Internal("printer worker stopped".into()))?
    }

    pub(crate) fn print_test_inner(
        driver: CustomUsbDriver,
        request: PrinterTestSchema,
        print_id: String,
    ) -> Result<(), PrinterError> {
        if request.test_page() {
            let test_commands = vec![
                Command::Smoothing(true),
                Command::Bold(true),
                Command::Underline(UnderlineMode::Single),
                Command::Writeln("Bold underline".to_string()),
                Command::Justify(JustifyMode::CENTER),
                Command::Reverse(true),
                Command::Bold(false),
                Command::Writeln("Hello world - Reverse".to_string()),
                Command::Feed(true),
                Command::Justify(JustifyMode::RIGHT),
                Command::Reverse(false),
                Command::Underline(UnderlineMode::None),
                Command::Size((2, 3)),
                Command::Writeln("Hello world - Normal".to_string()),
                Command::PrintCut(None),
            ];
            Self::execute_commands_inner(driver.clone(), test_commands, print_id.clone())?;
        }

        if !request.test_line().is_empty() {
            let line_commands = vec![
                Command::Writeln(request.test_line().to_string()),
                Command::PrintCut(None),
            ];
            Self::execute_commands_inner(driver, line_commands, print_id)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod owner_tests {
    use super::*;
    use crate::app::PrintLog;
    use crate::routes::routes;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn retry_backoff_is_positive_and_bounded_to_ten_percent() {
        for (base, expected_max) in [(1, 2), (2, 3), (5, 6), (10, 11), (30, 33)] {
            let delay = retry_delay(base, 9).as_secs();
            assert!(delay >= base);
            assert!(delay <= expected_max);
        }
    }

    #[test]
    fn successful_write_publishes_fresh_consumer_state_until_writes_stop() {
        let mut clock = super::super::health::FakeClock::new();
        let health = Arc::new(RwLock::new(HealthSnapshot::initial(&clock)));
        health.write().unwrap().transport = TransportState::Available;
        let publication = HealthPublication::new();
        let (status_tx, mut status_rx) = watch::channel(false);
        *publication.status_tx.write().unwrap() = Some(status_tx);

        clock.advance(Duration::from_secs(16));
        publish_successful_write(&health, &publication, &clock);
        assert!(*status_rx.borrow_and_update());

        clock.advance(Duration::from_secs(16));
        let stale = health.read().unwrap().refreshed(&clock);
        assert_eq!(stale.transport, TransportState::Degraded);

        publish_successful_write(&health, &publication, &clock);
        assert!(*status_rx.borrow_and_update());
    }

    #[test]
    fn cancellation_interrupts_owner_backoff() {
        let stop = AtomicBool::new(false);
        stop.store(true, Ordering::Release);
        assert!(!wait_retry(&stop, Duration::from_secs(30)));
    }

    struct RecordingBackend {
        events: Arc<std::sync::Mutex<Vec<&'static str>>>,
        active: Arc<AtomicUsize>,
    }

    impl OwnerBackend for RecordingBackend {
        fn presence(&mut self) -> Result<Presence, HealthError> {
            self.events.lock().unwrap().push("presence");
            Ok(Presence::Present)
        }

        fn print(
            &mut self,
            _commands: Vec<Command>,
            _reprint: bool,
            _print_id: String,
        ) -> Result<(), PrinterError> {
            assert_eq!(self.active.fetch_add(1, Ordering::SeqCst), 0);
            self.events.lock().unwrap().push("write-start");
            std::thread::sleep(Duration::from_millis(100));
            self.events.lock().unwrap().push("write-end");
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }

        fn print_test(
            &mut self,
            _request: PrinterTestSchema,
            _print_id: String,
        ) -> Result<(), PrinterError> {
            Ok(())
        }

        fn release(&mut self) -> Result<(), HealthError> {
            self.events.lock().unwrap().push("release");
            Ok(())
        }
    }

    fn worker() -> (
        Arc<BlockingUsbWorker>,
        Arc<std::sync::Mutex<Vec<&'static str>>>,
    ) {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let factory_events = events.clone();
        let factory_active = active.clone();
        let factory: OwnerFactory = Arc::new(move || {
            Ok(Box::new(RecordingBackend {
                events: factory_events.clone(),
                active: factory_active.clone(),
            }) as Box<dyn OwnerBackend>)
        });
        let health = Arc::new(RwLock::new(HealthSnapshot::initial(&SystemClock)));
        (
            BlockingUsbWorker::start(factory, health, HealthPublication::new()),
            events,
        )
    }

    #[test]
    fn production_owner_serializes_jobs_on_one_backend() {
        let (worker, events) = worker();
        let mut replies = Vec::new();
        for _ in 0..2 {
            let (reply, result) = tokio::sync::oneshot::channel();
            worker
                .submit(WorkerRequest::Print {
                    commands: Commands { commands: vec![] },
                    reprint: false,
                    reply,
                })
                .unwrap();
            replies.push(result);
        }
        std::thread::sleep(Duration::from_millis(250));
        for mut reply in replies {
            assert!(matches!(reply.try_recv(), Ok(Ok(()))));
        }
        let events = events.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .copied()
                .filter(|event| event.starts_with("write"))
                .collect::<Vec<_>>(),
            vec!["write-start", "write-end", "write-start", "write-end"]
        );
    }

    #[tokio::test]
    async fn maintenance_releases_before_ack_and_drop_resumes_discovery() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let factory_events = events.clone();
        let factory_active = active.clone();
        let factory: OwnerFactory = Arc::new(move || {
            Ok(Box::new(RecordingBackend {
                events: factory_events.clone(),
                active: factory_active.clone(),
            }) as Box<dyn OwnerBackend>)
        });
        let health = Arc::new(RwLock::new(HealthSnapshot::initial(&SystemClock)));
        let publication = HealthPublication::new();
        let worker = BlockingUsbWorker::start(factory, health.clone(), publication.clone());
        let service = PrinterService {
            worker: Arc::new(RwLock::new(Some(worker))),
            health,
            publication,
            usb_config: UsbConfig {
                vendor_id: 0,
                product_id: 0,
                endpoint: None,
                interface: None,
            },
            status_tx: None,
            sensor_tx: None,
            transport: None,
        };

        let guard = service.enter_maintenance().await.unwrap();
        assert_eq!(events.lock().unwrap().last().copied(), Some("release"));
        assert_eq!(
            service.health_snapshot().transport,
            TransportState::Disconnected
        );
        assert!(matches!(
            service.enter_maintenance().await,
            Err(AppError::Busy(_))
        ));
        std::thread::sleep(Duration::from_millis(75));
        assert_eq!(
            service.health_snapshot().transport,
            TransportState::Disconnected
        );
        drop(guard);
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if service.health_snapshot().transport == TransportState::Available {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("maintenance lease drop did not resume discovery");
    }

    #[test]
    fn production_owner_enforces_bounded_admission() {
        let (worker, _events) = worker();
        let mut accepted = 0;
        let mut replies = Vec::new();
        for _ in 0..40 {
            let (reply, result) = tokio::sync::oneshot::channel();
            if worker
                .submit(WorkerRequest::Print {
                    commands: Commands { commands: vec![] },
                    reprint: false,
                    reply,
                })
                .is_ok()
            {
                accepted += 1;
                replies.push(result);
            }
        }
        assert!(accepted <= 33, "active job plus 32 queued jobs");
    }

    struct BlockedBackend {
        started: Arc<AtomicBool>,
        release: Arc<AtomicBool>,
    }

    impl OwnerBackend for BlockedBackend {
        fn presence(&mut self) -> Result<Presence, HealthError> {
            Ok(Presence::Present)
        }

        fn print(
            &mut self,
            _commands: Vec<Command>,
            _reprint: bool,
            _print_id: String,
        ) -> Result<(), PrinterError> {
            self.started.store(true, Ordering::Release);
            while !self.release.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(())
        }

        fn print_test(
            &mut self,
            _request: PrinterTestSchema,
            _print_id: String,
        ) -> Result<(), PrinterError> {
            Ok(())
        }

        fn release(&mut self) -> Result<(), HealthError> {
            Ok(())
        }
    }

    struct ReleaseFailBackend {}

    impl OwnerBackend for ReleaseFailBackend {
        fn presence(&mut self) -> Result<Presence, HealthError> {
            Ok(Presence::Present)
        }

        fn print(
            &mut self,
            _commands: Vec<Command>,
            _reprint: bool,
            _print_id: String,
        ) -> Result<(), PrinterError> {
            Ok(())
        }

        fn print_test(
            &mut self,
            _request: PrinterTestSchema,
            _print_id: String,
        ) -> Result<(), PrinterError> {
            Ok(())
        }

        fn release(&mut self) -> Result<(), HealthError> {
            Err(HealthError::new(ErrorCategory::Busy, "release blocked"))
        }
    }

    #[tokio::test]
    async fn maintenance_release_error_does_not_leave_owner_paused() {
        let opens = Arc::new(AtomicUsize::new(0));
        let factory_opens = opens.clone();
        let factory: OwnerFactory = Arc::new(move || {
            factory_opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(ReleaseFailBackend {}) as Box<dyn OwnerBackend>)
        });
        let health = Arc::new(RwLock::new(HealthSnapshot::initial(&SystemClock)));
        let publication = HealthPublication::new();
        let worker = BlockingUsbWorker::start(factory, health.clone(), publication.clone());
        let service = PrinterService {
            worker: Arc::new(RwLock::new(Some(worker))),
            health,
            publication,
            usb_config: UsbConfig {
                vendor_id: 0,
                product_id: 0,
                endpoint: None,
                interface: None,
            },
            status_tx: None,
            sensor_tx: None,
            transport: None,
        };

        assert!(matches!(
            service.enter_maintenance().await,
            Err(AppError::Internal(_))
        ));
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline && opens.load(Ordering::Acquire) < 2 {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(opens.load(Ordering::Acquire) >= 2);
    }

    #[tokio::test]
    async fn maintenance_refuses_an_active_receipt_without_waiting_for_usb() {
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let factory_started = started.clone();
        let factory_release = release.clone();
        let factory: OwnerFactory = Arc::new(move || {
            Ok(Box::new(BlockedBackend {
                started: factory_started.clone(),
                release: factory_release.clone(),
            }) as Box<dyn OwnerBackend>)
        });
        let health = Arc::new(RwLock::new(HealthSnapshot::initial(&SystemClock)));
        let publication = HealthPublication::new();
        let worker = BlockingUsbWorker::start(factory, health.clone(), publication.clone());
        let service = PrinterService {
            worker: Arc::new(RwLock::new(Some(worker))),
            health,
            publication,
            usb_config: UsbConfig {
                vendor_id: 0,
                product_id: 0,
                endpoint: None,
                interface: None,
            },
            status_tx: None,
            sensor_tx: None,
            transport: None,
        };
        let print = tokio::spawn({
            let service = service.clone();
            async move {
                service
                    .execute_commands(Commands { commands: vec![] })
                    .await
            }
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while !started.load(Ordering::Acquire) && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let result = tokio::time::timeout(Duration::from_millis(100), service.enter_maintenance())
            .await
            .expect("busy maintenance must acknowledge promptly");
        assert!(matches!(result, Err(AppError::Busy(_))));
        release.store(true, Ordering::Release);
        let _ = print.await;
    }

    #[tokio::test]
    async fn production_owner_keeps_read_only_routes_under_blocked_usb_write() {
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let factory_started = started.clone();
        let factory_release = release.clone();
        let factory: OwnerFactory = Arc::new(move || {
            Ok(Box::new(BlockedBackend {
                started: factory_started.clone(),
                release: factory_release.clone(),
            }) as Box<dyn OwnerBackend>)
        });
        let health = Arc::new(RwLock::new(HealthSnapshot::initial(&SystemClock)));
        let publication = HealthPublication::new();
        let worker = BlockingUsbWorker::start(factory, health.clone(), publication.clone());
        let service = PrinterService {
            worker: Arc::new(RwLock::new(Some(worker))),
            health,
            publication,
            usb_config: UsbConfig {
                vendor_id: 0,
                product_id: 0,
                endpoint: None,
                interface: None,
            },
            status_tx: None,
            sensor_tx: None,
            transport: None,
        };
        let api = routes(
            service.clone(),
            Arc::new(std::sync::Mutex::new(PrintLog::new(4))),
        );
        let print = tokio::spawn({
            let service = service.clone();
            async move {
                service
                    .execute_commands(Commands { commands: vec![] })
                    .await
            }
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while !started.load(Ordering::Acquire) && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(started.load(Ordering::Acquire));
        assert!(service.health_snapshot().busy);
        for path in ["/health", "/print/test", "/printer/status"] {
            let begin = Instant::now();
            let response = tokio::time::timeout(
                Duration::from_millis(250),
                warp::test::request().method("GET").path(path).reply(&api),
            )
            .await
            .expect("read-only endpoint must not wait on native USB");
            assert!(response.status().is_success(), "{path}");
            assert!(begin.elapsed() < Duration::from_millis(250), "{path}");
        }
        let mut queued = Vec::new();
        for _ in 0..32 {
            let service = service.clone();
            queued.push(tokio::spawn(async move {
                service
                    .execute_commands(Commands { commands: vec![] })
                    .await
            }));
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        for (path, body) in [
            ("/print", serde_json::json!({"commands": []})),
            ("/print/reprint", serde_json::json!({"commands": []})),
        ] {
            let response = tokio::time::timeout(
                Duration::from_millis(250),
                warp::test::request()
                    .method("POST")
                    .path(path)
                    .json(&body)
                    .reply(&api),
            )
            .await
            .expect("full queue must reject immediately");
            assert_eq!(
                response.status(),
                warp::http::StatusCode::SERVICE_UNAVAILABLE
            );
            let json: serde_json::Value = serde_json::from_slice(response.body()).unwrap();
            assert_eq!(json["is_connected"], false);
            assert!(json["error"].as_str().unwrap().contains("busy"));
        }
        release.store(true, Ordering::Release);
        let _ = print.await;
        for task in queued {
            let _ = task.await;
        }
        assert!(!service.health_snapshot().busy);
    }

    struct FailingBackend;

    impl OwnerBackend for FailingBackend {
        fn presence(&mut self) -> Result<Presence, HealthError> {
            Ok(Presence::Present)
        }
        fn print(
            &mut self,
            _commands: Vec<Command>,
            _reprint: bool,
            _print_id: String,
        ) -> Result<(), PrinterError> {
            Err(PrinterError::Io("fake transfer failure".into()))
        }
        fn print_test(
            &mut self,
            _request: PrinterTestSchema,
            _print_id: String,
        ) -> Result<(), PrinterError> {
            Ok(())
        }
        fn release(&mut self) -> Result<(), HealthError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn production_owner_forwards_print_failure_to_sensor_channel() {
        let health = Arc::new(RwLock::new(HealthSnapshot::initial(&SystemClock)));
        let publication = HealthPublication::new();
        let (sensor_tx, mut sensor_rx) = tokio::sync::mpsc::channel(4);
        *publication.sensor_tx.write().unwrap() = Some(sensor_tx);
        let factory: OwnerFactory =
            Arc::new(|| Ok(Box::new(FailingBackend) as Box<dyn OwnerBackend>));
        let worker = BlockingUsbWorker::start(factory, health.clone(), publication.clone());
        let (reply, result) = tokio::sync::oneshot::channel();
        worker
            .submit(WorkerRequest::Print {
                commands: Commands { commands: vec![] },
                reprint: false,
                reply,
            })
            .unwrap();
        let event = tokio::time::timeout(Duration::from_secs(1), sensor_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, SensorEvent::PrintFail(_)));
        drop(result);
    }

    struct CountedFailingBackend {
        prints: Arc<AtomicUsize>,
        releases: Arc<AtomicUsize>,
    }

    impl OwnerBackend for CountedFailingBackend {
        fn presence(&mut self) -> Result<Presence, HealthError> {
            Ok(Presence::Present)
        }

        fn print(
            &mut self,
            _commands: Vec<Command>,
            _reprint: bool,
            _print_id: String,
        ) -> Result<(), PrinterError> {
            self.prints.fetch_add(1, Ordering::SeqCst);
            Err(PrinterError::Io("ambiguous fake write failure".into()))
        }

        fn print_test(
            &mut self,
            _request: PrinterTestSchema,
            _print_id: String,
        ) -> Result<(), PrinterError> {
            Ok(())
        }

        fn release(&mut self) -> Result<(), HealthError> {
            self.releases.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn failed_http_job_is_returned_once_and_owner_is_released_for_recovery() {
        let opens = Arc::new(AtomicUsize::new(0));
        let prints = Arc::new(AtomicUsize::new(0));
        let releases = Arc::new(AtomicUsize::new(0));
        let factory_opens = opens.clone();
        let factory_prints = prints.clone();
        let factory_releases = releases.clone();
        let factory: OwnerFactory = Arc::new(move || {
            factory_opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(CountedFailingBackend {
                prints: factory_prints.clone(),
                releases: factory_releases.clone(),
            }) as Box<dyn OwnerBackend>)
        });
        let health = Arc::new(RwLock::new(HealthSnapshot::initial(&SystemClock)));
        let publication = HealthPublication::new();
        let worker = BlockingUsbWorker::start(factory, health.clone(), publication.clone());
        let service = PrinterService {
            worker: Arc::new(RwLock::new(Some(worker))),
            health,
            publication,
            usb_config: UsbConfig {
                vendor_id: 0,
                product_id: 0,
                endpoint: None,
                interface: None,
            },
            status_tx: None,
            sensor_tx: None,
            transport: None,
        };

        let result = service
            .execute_commands(Commands { commands: vec![] })
            .await;
        assert!(
            matches!(result, Err(AppError::PrinterError(message)) if message.contains("ambiguous fake write failure"))
        );
        assert_eq!(prints.load(Ordering::SeqCst), 1);
        assert_eq!(releases.load(Ordering::SeqCst), 1);

        let deadline = Instant::now() + Duration::from_secs(2);
        while opens.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            opens.load(Ordering::SeqCst) >= 2,
            "background discovery did not reopen owner"
        );
        assert_eq!(
            prints.load(Ordering::SeqCst),
            1,
            "failed job must not be replayed during recovery"
        );
    }
}
