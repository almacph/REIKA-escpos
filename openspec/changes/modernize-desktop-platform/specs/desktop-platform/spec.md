## ADDED Requirements

### Requirement: Reproducible Validated Releases

Release builds SHALL use a committed lockfile and a pinned supported Rust toolchain. Publication SHALL require formatting, build, unit/contract checks, a dependency advisory review, and native Windows validation in addition to the existing cross-build. Releases SHALL identify source commit, toolchain, application version, and artifact hashes. Unresolved applicable vulnerabilities SHALL block release unless covered by a documented time-limited exception.

#### Scenario: Failed release check
- **WHEN** a required check fails
- **THEN** the release artifact is not published

#### Scenario: Dependency group upgrade
- **WHEN** dependencies are upgraded
- **THEN** the resulting lockfile, minimum compiler requirement, and API/config/printing regressions are checked before publication

### Requirement: Restart-Safe Diagnostic Retention

Enabled debug logging SHALL append or rotate instead of truncating on startup, with a 5 MiB active-file limit and at most three rotated files. Oversized individual records SHALL be truncated with a marker to preserve the bound. Diagnostic exports SHALL omit API keys and receipt text/command payloads by default. Logging disabled SHALL continue to suppress debug-file creation.

#### Scenario: Watchdog restart
- **WHEN** the process restarts after a failure with logging enabled
- **THEN** preceding diagnostic entries remain available within the retention limits

#### Scenario: Support export
- **WHEN** an operator exports diagnostics
- **THEN** version, state transitions, and error categories are included without credentials or receipt contents
