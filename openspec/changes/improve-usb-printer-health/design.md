## Evidence

- `src/server.rs`: calls check_connection every 30 seconds.
- `src/services/printer.rs::check_connection/try_init`: sends printer.init; a successful write becomes true without reading status. Failed checks only publish false and return; no idle reconnect is scheduled.
- `with_retry`: clones a driver before invoking the print operation; the outer async mutex is not a whole-receipt lock. `CustomUsbDriver` locks each native read/write separately, so another request/probe can run between command calls.
- `reconnect`: waits for initialize_device_with_config while self.driver still retains the old handle. Multiple callers can attempt independent reconnects.
- `src/services/usb_driver.rs`: synchronous USB calls and claim sleeps; auto-discovery requires both bulk IN and OUT, manual mode assumes IN = OUT | 0x80, descriptor selection does not retain an alternate-setting choice, and typed rusb errors are collapsed into strings.
- `src/main.rs`: HTTP startup is conditional on first USB open. `src/app/gui.rs` and sensor reporter reduce health to ONLINE/OFFLINE, and do not distinguish unknown readiness.

These explain plausible failure modes but require fake-backend tests and physical reproduction before claiming an RCA.

## Health model

Maintain one snapshot with `transport` = discovering, available, disconnected, reconnecting, driver_error, or degraded; `readiness` = ready, paper_out, cover_open, printer_error, or unknown; `busy`; `status_capability` = verified or unsupported; `reason`; connection generation; and timestamps for last transport observation, last readiness response, and last successful write. Use monotonic time for age/timeout decisions, wall-clock timestamps for support output. Never use the last successful print as permanent proof of present readiness.

Service `/health` means the HTTP/control plane responds, independent of USB. `/print/test` remains a cheap compatibility endpoint returning HTTP 200 and the existing boolean/error fields from the snapshot. `is_connected` is true only for fresh available transport with no known blocking physical fault; unsupported readiness can remain true but is explicitly unknown in `/printer/status` and the GUI. This preserves output-only printers without claiming paper is present. Unknown/stale/unavailable transport maps to false. A busy but healthy printer does not turn offline merely because no idle readiness probe ran.

## One owner and serialization

Use one long-lived dedicated blocking USB worker and a bounded in-memory job channel (proposed capacity: 32 pending requests). It owns the native handle, current generation, descriptor selection, complete print attempts, status transactions, and recovery. HTTP and GUI read an immutable cached snapshot and never wait for a native call. Native USB I/O must not run on Tokio runtime workers. Do not simulate cancellation by timing out an async wrapper and spawning another blocking operation against the same device.

Serialize entire print attempts, including init and final flush/cut. Status transactions are idle-only and never interleave with a receipt or raster data. Coalesce due probes into one pending flag so frequent HTTP status requests cannot fill the job channel. When the admission channel is full, reject new print work with HTTP 503 and the existing JSON error fields before output; GUI submissions receive an explicit busy error. Keep accepted jobs FIFO. The active job retains its place during existing indefinite recoverable retry; expose that waiting state instead of silently claiming throughput or inventing cancellation. This is not a durable job system.

## Probe policy and freshness

- No periodic Init, Reset, feed, cut, drawer pulse, or print-test output. Init remains allowed as part of an actual print job.
- Default device presence sampling every 5 seconds while idle. OS arrival/removal notifications may wake the worker, but never replace periodic reconciliation. Check backend capability before relying on libusb hotplug; native Windows notifications or enumeration remain the fallback.
- Detailed physical status is opt-in per validated printer profile, with descriptor-backed bulk IN and documented protocol support. For verified ESC/POS profiles, query DLE EOT statuses sequentially and validate response length/marker bits and profile-specific meanings. One request/response at a time; do not send following bytes until the response or bounded timeout is handled. Do not enable Automatic Status Back without a dedicated parser. Unknown clones and output-only devices default to readiness unknown.
- Default readiness sampling every 15 seconds while idle, 250 ms maximum per transaction and 1 second total per cycle. An optional status timeout is not proof of unplugging; retain transport availability, mark readiness unknown/degraded, and disable repeated status probing for the connection after three consecutive unsupported/malformed/timeout results. A new connection or explicit diagnostic retest can retry the capability.
- A supported status response may reveal paper-out, cover-open, or another physical fault immediately. Do not restart the process or repeatedly rebind the driver for a physical fault; wait for its clearance.
- Soft transport-probe failures require two consecutive failed samples before declaring unavailable. A definite NoDevice/removal, failed print write, or partial/zero write immediately invalidates that transport generation. A successful open/presence reconciliation clears disconnected state but does not assert physical readiness.
- A transport observation older than 15 seconds is stale unless bounded successful writes are actively refreshing transport evidence. Readiness can independently become unknown after 30 seconds, including during long jobs. GET endpoints return immediately with age metadata; they may request one coalesced future refresh.

## Endpoint discovery and identity

Select descriptors from one interface and alternate setting, require a valid bulk OUT endpoint, and treat bulk IN as optional. Validate manual OUT/interface selections against descriptors and discover the actual compatible IN endpoint independently. Never derive it by OR-ing 0x80. If a non-default alternate setting is selected, activate it explicitly before I/O. Multiple candidates require a deterministic validated profile or explicit selection, not “first device wins.” Keep the selected serial/port identity for the session; after disconnect do not switch to another identical printer. If identity cannot be recovered unambiguously, expose selection required. Persisted serial selection is a follow-up configuration feature.

## Recovery lifecycle

On a hard failure, invalidate the generation, stop using it, release claimed interfaces/handles and all clones, then reopen. A single worker owns recovery even when a print, HTTP poll, and sensor event observe the same fault. Reconnect occurs without a pending print. Retry at 1, 2, 5, 10, then 30 seconds with up to 10 percent positive jitter; a verified arrival event can wake it sooner. Stop the recovery loop on application shutdown and clear pending maintenance cleanly. Permission/driver-binding errors expose setup guidance and use the slow retry cadence; they are not treated as paper-out or fixed by repeated blind clear-halt calls. Clear endpoint halt only as part of controlled open/stall recovery. A successful new connection resets backoff. Late observations tagged with old generations cannot overwrite current state.

Retain typed native errors long enough to distinguish NoDevice, Access, Busy, Timeout, Pipe, and invalid input. Keep the existing USB transfer timeout of 5 seconds for print writes; health has its shorter explicit budget. A libusb timeout can occur after partial transfer; treat delivery as uncertain and do not claim exactly-once output. Existing whole-job retry can still duplicate content, and the current client can fall back to Bluetooth. This proposal makes those limitations visible but does not change consumer delivery semantics.

## Sensor and operator behavior

Broadcast a structured snapshot, derive the existing bool for compatibility consumers, and emit notifications only on meaningful state transitions. SensorReporter initializes from the current snapshot and continues 60-second heartbeats; status events must not reset the heartbeat timer. Map available/no-known-fault to ONLINE, unavailable or known blocking fault to OFFLINE, actual USB errors to USB_ERROR and print execution errors to PRINT_FAIL. Keep error event reporting but publish recovery after a fresh successful transport observation instead of leaving an error latched indefinitely. Unsupported readiness alone is not USB_ERROR. Do not add new wire sensor values without coordinating REIKA.

Show USB connected versus printer ready, last checked age, busy/retrying state, and actionable driver/paper/cover/cable reasons. Retain preset/VID/PID/endpoint details. A successful transfer says data was sent, not that paper emerged.

## Validation and success criteria

Fake-backend tests must verify no Init during health checks; no probe/job or job/job interleaving; output-only and asymmetric endpoints; stale generation suppression; bounded retry cadence; absent-printer startup; idle unplug/replug and power cycle; no false offline from unsupported status; paper/cover transitions; and sensor heartbeat/recovery. Under a fake 5-second blocked USB write, both status HTTP endpoints must answer within a 250 ms test budget on the test runner. Native hardware validation records actual response bytes per preset before enabling its profile. Include concurrent print/status load, long raster output, unplug during transfer, permissions/driver mismatch, shutdown while retrying, and watchdog maintenance. Physical tests are a release requirement, not implied by unit tests.

## Primary references

- [Epson DLE EOT reference](https://download4.epson.biz/sec_pubs/pos/reference_en/escpos/dle_eot.html): status meanings and response ordering are model-specific; this does not establish compatibility for the two third-party presets.
- [libusb synchronous I/O](https://libusb.sourceforge.io/api-1.0/group__libusb__syncio.html): transfer results/timeouts and partial transfers.
- [libusb hotplug capability](https://libusb.sourceforge.io/api-1.0/group__libusb__hotplug.html): capability detection and event handling.
- [libusb Windows restrictions](https://github.com/libusb/libusb/wiki/Windows): backend and driver limitations.

Timing thresholds, queue capacity, and profile policy above are proposed application defaults, not values mandated by those sources.
