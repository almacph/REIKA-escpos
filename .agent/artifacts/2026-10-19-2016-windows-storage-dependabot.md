status: complete
confidence: high
scope: Windows atomic state replacement and Cargo Dependabot configuration

## Summary
- Windows CI's `PermissionDenied` came from retaining a read handle on the temporary file while attempting replacement.
- The previous existing-file path also moved the live file aside before installing the new one, creating a target gap.
- Cargo Dependabot now uses the supported `auto` strategy.

## Evidence
- `src/app/storage.rs:157-165` - temporary file is synced and explicitly dropped before replacement.
- `src/app/storage.rs:173-203` - Unix uses atomic rename; Windows uses `ReplaceFileW` with the documented zero flags and maps the Win32 status to `io::Error`.
- `.github/dependabot.yml:8` - Cargo `versioning-strategy` is `auto`.
- Microsoft Learn `ReplaceFileW` documentation states `REPLACEFILE_WRITE_THROUGH` is unsupported and that replacement retains original names on failure.

## Impacted Files
- `src/app/storage.rs`
- `.github/dependabot.yml`

## Risks
- Windows runtime behavior cannot be executed in this Linux environment; Windows GNU cross compilation and test compilation passed.
- Existing `last-good` copy remains best effort as before.

## Applied Changes
- `src/app/storage.rs` - close the synced temp-file handle before replacement; use platform-specific atomic replacement without a delete-target gap; preserve last-good behavior.
- `.github/dependabot.yml` - change Cargo strategy from unsupported `increase-if-necessary` to `auto`.

## Validation
- `cargo fmt --all -- --check` - pass.
- `REIKA_NATIVE_DEPS=/tmp/reika-native-deps.vLV4dr/root .agent/run-native-tests.sh` - pass, 64 tests.
- `cargo check --locked --target x86_64-pc-windows-gnu` - pass.
- `cargo test --locked --target x86_64-pc-windows-gnu --no-run` - pass.
- `cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets --all-features -- -D warnings` - blocked by two pre-existing lints in `src/bin/reika-driver-setup.rs`.
- `git diff --check` - pass.

## Recommended Action
1. Run the Windows runtime test job to confirm the corrected handle lifetime under native Windows.

## Open Questions
- None
