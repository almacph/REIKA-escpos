status: complete
confidence: high
scope: Windows watchdog VBScript parser regression and CI self-test

## Summary
- Fixed the VBScript compilation error at the liveness decision by replacing the invalid compressed `If ... ElseIf` statement with a multiline VBScript conditional.
- Added a parser/runtime self-test mode that exits before COM creation, filesystem mutation, process inspection, or the watchdog loop.
- Added pure liveness policy and JSON serialization assertions and wired the self-test into the Windows CI job.

## Evidence
- `reika-watchdog.vbs:6-10` - `--self-test` is detected before `WScript.Shell` and `FileSystemObject` creation.
- `reika-watchdog.vbs:64-79` - liveness classification uses valid multiline VBScript syntax.
- `reika-watchdog.vbs:131-168` - pure JSON/policy helpers and assertions cover HTTP error, non-200, invalid body, healthy body, and serialized JSON output.
- `.github/workflows/release.yml:45-50` - Windows runner executes `cscript //nologo reika-watchdog.vbs --self-test`.

## Impacted Files
- `reika-watchdog.vbs`
- `.github/workflows/release.yml`

## Risks
- CI validates parser/runtime behavior and pure policy/serialization only; live process termination, HTTP probing, and scheduled-task behavior still require Windows deployment/hardware validation.

## Recommended Action
1. Commit and push the two-file fix to `main`.
2. Require the new Windows CI step before accepting subsequent watchdog changes.

## Open Questions
- None

## Applied Changes
- `reika-watchdog.vbs` - split invalid `ElseIf` chain, extracted liveness and JSON helpers, and added `--self-test`.
- `.github/workflows/release.yml` - added the Windows Script Host regression gate.

## Validation
- `/mnt/c/Windows/System32/cscript.exe //nologo "$(wslpath -w "$PWD/reika-watchdog.vbs")" --self-test` - pass on actual Windows Script Host; output: `reika-watchdog.vbs self-test passed`.
- `git diff --check` - pass.
