# Windows installer foundation

The selected installer format is an **Inno Setup EXE**. It supports an
elevated setup phase, protected application files, user-logon task registration,
repair, and uninstall while keeping mutable state in the original user's state
directory. The final installer must invoke the narrow signed driver helper and
must verify the signed INF/CAT before any binding operation.

The production package remains blocked on the physical composite-interface
capture, trusted driver-package signing identity, and representative Windows
hardware validation. Until those inputs exist, the setup experience must show
diagnosis or a clearly labelled recovery fallback rather than claim automatic
WinUSB installation.

`reika-setup.iss` installs under Program Files, registers `REIKA\Watchdog` at
interactive user logon, and removes that task during uninstall. The watchdog
uses bounded process and `/health` checks, a three-failure threshold, and a
five-minute restart cooldown. A fresh `maintenance.lease` in the original user state directory, rather than beside the installed
executable pauses recovery for at most 15 minutes during explicit repair.

The elevated installer never expands the administrator's `%LOCALAPPDATA%` for
application state. The scheduled task and `reika-user-launcher.cmd` run as the
original interactive user and set `REIKA_INSTALL_MODE=installed` plus
`REIKA_ORIGINAL_USER_LOCALAPPDATA=%LOCALAPPDATA%` in that user's environment.
