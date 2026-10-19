status: complete
confidence: high
scope: Windows strict Clippy gate cleanup

## Summary
- Resolved both strict Windows Clippy failures in the existing driver setup helper without changing behavior.
- Confirmed the storage replacement root cause was a read-only handle opened by `OpenOptions::new().read(true).open(&temp)`, which was still alive when Windows replacement began.

## Evidence
- `src/bin/reika-driver-setup.rs:181` - replaced manual ceil division with equivalent `div_ceil`.
- `src/bin/reika-driver-setup.rs:631` - removed redundant `false.into()`; the API accepts `bool` directly.
- `src/app/storage.rs:158-160` - read handle is synced, then dropped before replacement.

## Applied Changes
- `src/bin/reika-driver-setup.rs` - Clippy-only expression cleanups.

## Validation
- `cargo fmt --all -- --check` - pass.
- `cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets --all-features -- -D warnings` - pass.
- `cargo check --locked --target x86_64-pc-windows-gnu` - pass.
- `cargo test --locked --target x86_64-pc-windows-gnu --no-run` - pass.
- `REIKA_NATIVE_DEPS=/tmp/reika-native-deps.vLV4dr/root .agent/run-native-tests.sh` - pass, 64 tests.

## Risks
- Windows runtime execution remains unavailable in this Linux environment; CI Windows runtime coverage is still required.

## Recommended Action
1. Push and allow the Windows CI runtime test to execute the storage regression test.

## Open Questions
- None
