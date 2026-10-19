#define AppName "REIKA Printer Service"
#define AppVersion "2.0.0"
[Setup]
AppId={{A7F1D8A7-8F0C-4A09-9A34-9E1A5C0E4D71}}
AppName={#AppName}
AppVersion={#AppVersion}
DefaultDirName={autopf64}\REIKA-escpos
DisableDirPage=yes
PrivilegesRequired=admin
ArchitecturesInstallIn64BitMode=x64
OutputBaseFilename=reika-setup
Uninstallable=yes
DisableProgramGroupPage=yes
[Files]
Source: "..\target\x86_64-pc-windows-gnu\release\reika-escpos.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "reika-user-launcher.cmd"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\reika-watchdog.vbs"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\x86_64-pc-windows-gnu\release\reika-driver-setup.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "driver\*.inf"; DestDir: "{app}\driver"; Flags: ignoreversion; Check: SignedDriverAssetsReady
Source: "driver\*.cat"; DestDir: "{app}\driver"; Flags: ignoreversion signcheck; Check: SignedDriverAssetsReady
Source: "driver\supported-devices.toml"; DestDir: "{app}\driver"; Flags: ignoreversion
Source: "driver\signing-provenance.json"; DestDir: "{app}\driver"; Flags: ignoreversion; Check: SignedDriverAssetsReady
[Run]
; Registration runs in the original user's context, so /RU is never the
; elevated administrator account. The launcher also supplies that user's state.
Filename: "{app}\reika-user-launcher.cmd"; Parameters: "register"; Flags: postinstall shellexec runasoriginaluser waituntilterminated
; The repair entry is explicit and user initiated; the GUI owns device selection
; and passes the canonical instance to the elevated helper. No arbitrary device
; is auto-bound during setup.
Filename: "{app}\reika-escpos.exe"; Parameters: "--driver-setup"; Flags: postinstall shellexec runasoriginaluser skipifsilent
Filename: "{app}\reika-user-launcher.cmd"; Flags: postinstall shellexec runasoriginaluser skipifsilent
[UninstallRun]
Filename: "{sys}\schtasks.exe"; Parameters: "/Delete /F /TN REIKA\Watchdog"; Flags: runhidden waituntilterminated
[Code]
function SignedDriverAssetsReady(): Boolean;
begin
  Result := FileExists(ExpandConstant('{src}\driver\standard.inf')) and
    FileExists(ExpandConstant('{src}\driver\standard.cat')) and
    FileExists(ExpandConstant('{src}\driver\ics-advent.inf')) and
    FileExists(ExpandConstant('{src}\driver\ics-advent.cat')) and
    FileExists(ExpandConstant('{src}\driver\signing-provenance.json'));
end;
