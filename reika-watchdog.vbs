' REIKA Printer Service Watchdog (Hidden)
' Place this in shell:startup - runs completely invisible
' Restarts the service if it crashes

Set WshShell = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")

' Get the directory where this script is located
scriptPath = fso.GetParentFolderName(WScript.ScriptFullName)
exePath = scriptPath & "\reika-escpos.exe"

' Check interval in seconds
checkInterval = 30

Do While True
    ' Check if process is running
    Set objWMI = GetObject("winmgmts:\\.\root\cimv2")
    Set processes = objWMI.ExecQuery("SELECT * FROM Win32_Process WHERE Name = 'reika-escpos.exe'")

    If processes.Count = 0 Then
        ' Not running, start it (hidden)
        WshShell.Run """" & exePath & """", 1, False
    End If

    ' Wait before next check
    WScript.Sleep checkInterval * 1000
Loop
