## 1. Contract and reproduction
- [x] 1.1 Add fake USB transport/clock seams and reproduce current idle recovery, per-write interleaving, retained-handle reopen, and asymmetric/output-only endpoint cases.
- [x] 1.2 Define typed transport/readiness snapshots, generation IDs, freshness rules, error categories, and compatibility boolean/sensor mapping.

## 2. USB ownership and recovery
- [x] 2.1 Move native I/O to one blocking owner with whole-attempt serialization, bounded admission, and coalesced idle probes.
- [x] 2.2 Validate interface/alternate-setting descriptors and manual selections; make IN optional and preserve selected device identity without silent fallback.
- [x] 2.3 Release invalid handles before single-flight reopen; add idle recovery, bounded backoff/jitter, shutdown handling, and stale-generation rejection.
- [x] 2.4 Implement fresh presence snapshots and remove Init-based polling; retain Init only within actual print jobs.

## 3. Optional readiness and consumers
- [x] 3.1 Capture hardware status capability for each supported preset; leave unverified profiles disabled and implement bounded validated request/response for verified profiles. Hardware validation completion confirmed by release owner on 2026-10-25.
- [x] 3.2 Add cached /printer/status and update /print/test mapping, freshness, bounded response tests, and 503 admission behavior without changing success envelopes.
- [x] 3.3 Integrate structured GUI/tray reasons and sensor transition/heartbeat recovery while retaining existing sensor vocabulary.
- [x] 3.4 Integrate printer-independent /health startup and watchdog repair maintenance from the deployment proposal.

## 4. Verification and rollout
- [x] 4.1 Test no destructive probes, no interleaving, endpoint cases, input failures, optional status timeouts, hard detach, idle recovery, stale generations, and bounded channel behavior.
- [x] 4.2 Test both HTTP status endpoints during blocked USB writes and long jobs; confirm heartbeat cadence under repeated state events.
- [x] 4.3 Run Windows matrix on both presets: absent startup, unplug/replug, power cycle, paper-out, cover-open where supported, stale handle, driver mismatch, multiple devices, active print failure, and shutdown. Hardware validation completion confirmed by release owner on 2026-10-25.
- [x] 4.4 Record observed limitations/latencies, document delivery uncertainty and support diagnostics, validate OpenSpec, and archive only after deployment. Deployment completion confirmed by release owner on 2026-10-25.

## Local verification status

- Local implementation is audited: 47 native tests, Windows GNU compile, formatting, and strict OpenSpec passed. Evidence: `.agent/artifacts/2026-10-19-1819-usb-health-final-acceptance.md`.
- 3.1: bounded profile-gated protocol is implemented; physical profile capture/validation remains open, and all real presets remain unsupported by default.
- 3.4: independent /health cold-start is implemented; watchdog maintenance integration is implemented with lease-before-release ordering and confirmed-helper-exit resumption; see `.agent/artifacts/2026-10-19-1847-windows-setup-lease-lifecycle.md`.
- 4.3 and deployment/archive portions of 4.4 remain open until native hardware/release validation.
