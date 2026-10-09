@echo off
setlocal EnableExtensions DisableDelayedExpansion
title RazerBatteryTaskbar HID Probe

pushd "%~dp0"
if errorlevel 1 (
    echo ERROR: Could not open the HID Probe directory.
    pause
    exit /b 1
)

set "PROBE=RazerBatteryTaskbar-HID-Probe.exe"
set "REPORT=razer-hid-report.json"
set "ERRORS=razer-hid-errors.txt"

if not exist "%PROBE%" (
    echo ERROR: %PROBE% was not found next to this launcher.
    echo Extract the entire RazerBatteryTaskbar-HID-Probe ZIP first.
    popd
    pause
    exit /b 1
)

echo RazerBatteryTaskbar HID Probe
echo ---------------------------
echo Running read-only HID enumeration...
echo.
"%PROBE%" --json 1>"%REPORT%" 2>"%ERRORS%"
set "EXIT_CODE=%ERRORLEVEL%"

echo.
echo Exit code: %EXIT_CODE%
if "%EXIT_CODE%"=="0" (
    echo Report saved: %CD%\%REPORT%
    echo Only VID/PID and HID collection metadata are included.
) else (
    echo The probe returned an error. Diagnostic output:
    if exist "%ERRORS%" type "%ERRORS%"
    echo.
    echo Partial report, if any: %CD%\%REPORT%
)

echo Error log: %CD%\%ERRORS%
echo.
echo This window will remain open until you press a key.
popd
pause
exit /b %EXIT_CODE%
