' REIKA Printer Service watchdog. Runs in the interactive user's session.
Option Explicit
Dim shell, fso, scriptPath, exePath, stateRoot, logPath, failures, lastRestart
If HasArgument("--self-test") Then
  RunSelfTest
  WScript.Quit 0
End If
Set shell = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")
scriptPath = fso.GetParentFolderName(WScript.ScriptFullName)
exePath = fso.BuildPath(scriptPath, "reika-escpos.exe")
stateRoot = shell.ExpandEnvironmentStrings("%REIKA_ORIGINAL_USER_LOCALAPPDATA%")
If stateRoot = "%REIKA_ORIGINAL_USER_LOCALAPPDATA%" Then stateRoot = shell.ExpandEnvironmentStrings("%LOCALAPPDATA%")
stateRoot = fso.BuildPath(stateRoot, "REIKA-escpos")
logPath = fso.BuildPath(stateRoot, "watchdog.log")
failures = 0
lastRestart = DateSerial(1970,1,1)
LogEvent "started", 0, "none", "none"
Do While True
  If MaintenanceLeaseActive(stateRoot) Then
    failures = 0
    LogEvent "lease_suppressed", 0, "none", "suppressed"
  Else
    Dim result
    result = LivenessResult(exePath)
    If result = "healthy" Then
      failures = 0
      If ProcessCount(exePath) > 1 Then LogEvent "duplicate_primary_preserved", ProcessCount(exePath), "healthy", "suppressed"
    Else
      failures = failures + 1
      LogEvent "liveness_failure", ProcessCount(exePath), result, "threshold=" & failures
      If failures >= 3 And DateDiff("s", lastRestart, Now) >= 300 Then
        If MaintenanceLeaseActive(stateRoot) Then
          LogEvent "restart_suppressed", ProcessCount(exePath), result, "lease_active"
        ElseIf fso.FileExists(exePath) Then
          Dim termination
          termination = TerminateExactProcesses(exePath)
          If termination = "terminated" Then
            shell.Run Chr(34) & exePath & Chr(34), 1, False
            LogEvent "restart_requested", ProcessCount(exePath), result, "terminated_then_launched"
          Else
            LogEvent "restart_skipped", ProcessCount(exePath), result, termination
          End If
        Else
          LogEvent "restart_skipped", 0, result, "missing_executable"
        End If
        lastRestart = Now
        failures = 0
      ElseIf failures >= 3 Then
        LogEvent "restart_cooldown", ProcessCount(exePath), result, "suppressed"
      End If
    End If
  End If
  WScript.Sleep 30000
Loop

Function LivenessResult(expectedPath)
  Dim count, http, status, body
  count = ProcessCount(expectedPath)
  If count = 0 Then LivenessResult = "process_missing": Exit Function
  On Error Resume Next
  Set http = CreateObject("WinHttp.WinHttpRequest.5.1")
  http.SetTimeouts 1000, 1000, 1500, 1500
  http.Open "GET", "http://127.0.0.1:55000/health", False
  http.Send
  status = http.Status
  body = LCase(http.ResponseText)
  LivenessResult = LivenessDecision(Err.Number, status, body)
  On Error GoTo 0
End Function

Function LivenessDecision(errorNumber, status, body)
  If errorNumber <> 0 Then
    LivenessDecision = "port_or_http_error"
  ElseIf status <> 200 Then
    LivenessDecision = "health_status_" & status
  ElseIf InStr(body, "healthy") = 0 And InStr(body, "service") = 0 Then
    LivenessDecision = "health_body_invalid"
  Else
    LivenessDecision = "healthy"
  End If
End Function

Function ProcessCount(expectedPath)
  Dim svc, processes, process, count
  count = 0
  On Error Resume Next
  Set svc = GetObject("winmgmts:\\.\root\cimv2")
  Set processes = svc.ExecQuery("SELECT ExecutablePath FROM Win32_Process WHERE Name='reika-escpos.exe'")
  For Each process In processes
    If LCase(process.ExecutablePath) = LCase(expectedPath) Then count = count + 1
  Next
  On Error GoTo 0
  ProcessCount = count
End Function

Function TerminateExactProcesses(expectedPath)
  Dim svc, processes, process, pid, deadline
  On Error Resume Next
  Set svc = GetObject("winmgmts:\\.\root\cimv2")
  Set processes = svc.ExecQuery("SELECT ProcessId, ExecutablePath FROM Win32_Process WHERE Name='reika-escpos.exe'")
  For Each process In processes
    If LCase(process.ExecutablePath) = LCase(expectedPath) Then
      pid = process.ProcessId
      process.Terminate 1
      LogEvent "terminate_requested", ProcessCount(expectedPath), "unhealthy", "pid=" & pid
    End If
  Next
  On Error GoTo 0
  deadline = DateAdd("s", 5, Now)
  Do While DateDiff("s", Now, deadline) >= 0
    If ProcessCount(expectedPath) = 0 Then
      TerminateExactProcesses = "terminated"
      Exit Function
    End If
    WScript.Sleep 250
  Loop
  TerminateExactProcesses = "termination_timeout"
End Function

Sub LogEvent(kind, count, health, decision)
  Dim folder, stream, line
  folder = fso.GetParentFolderName(logPath)
  If Not fso.FolderExists(folder) Then On Error Resume Next: fso.CreateFolder folder: On Error GoTo 0
  line = JsonLogLine(CStr(Now), kind, count, health, decision)
  On Error Resume Next
  Set stream = fso.OpenTextFile(logPath, 8, True)
  stream.WriteLine line
  stream.Close
  If fso.FileExists(logPath) Then If fso.GetFile(logPath).Size > 262144 Then fso.DeleteFile logPath, True
  On Error GoTo 0
End Sub

Function JsonLogLine(timestampValue, kind, count, health, decision)
  JsonLogLine = "{""time"":""" & Replace(Replace(CStr(timestampValue), """", "'"), " ", "T") & """,""event"":""" & kind & """,""process_count"":" & count & ",""health"":""" & health & """,""decision"":""" & decision & """}"
End Function

Function HasArgument(value)
  Dim argument
  HasArgument = False
  For Each argument In WScript.Arguments
    If LCase(CStr(argument)) = LCase(value) Then
      HasArgument = True
      Exit Function
    End If
  Next
End Function

Sub RunSelfTest()
  AssertEqual "port_or_http_error", LivenessDecision(5, 0, ""), "HTTP error classification"
  AssertEqual "health_status_503", LivenessDecision(0, 503, "service unavailable"), "HTTP status classification"
  AssertEqual "health_body_invalid", LivenessDecision(0, 200, "printer offline"), "health body classification"
  AssertEqual "healthy", LivenessDecision(0, 200, "{""healthy"":true}"), "healthy body classification"
  AssertEqual "{""time"":""2026-10-19T12:34:56"",""event"":""self_test"",""process_count"":0,""health"":""healthy"",""decision"":""none""}", JsonLogLine("2026-10-19 12:34:56", "self_test", 0, "healthy", "none"), "JSON log serialization"
  WScript.Echo "reika-watchdog.vbs self-test passed"
End Sub

Sub AssertEqual(expected, actual, label)
  If CStr(actual) <> CStr(expected) Then
    WScript.Echo "self-test failed: " & label & " (expected '" & expected & "', got '" & actual & "')"
    WScript.Quit 1
  End If
End Sub

Function MaintenanceLeaseActive(root)
  Dim lease, age
  MaintenanceLeaseActive = False
  If Not fso.FolderExists(root) Then Exit Function
  lease = fso.BuildPath(root, "maintenance.lease")
  If fso.FileExists(lease) Then
    age = DateDiff("s", fso.GetFile(lease).DateLastModified, Now)
    MaintenanceLeaseActive = (age >= 0 And age < 900)
  End If
End Function
