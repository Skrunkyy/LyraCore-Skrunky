@echo off
rem LyraCore development CLI launcher for Windows — the counterpart of the POSIX `./lyracore`
rem shim at this checkout's root.
rem
rem It walks up from the current directory for the checkout you are standing in (a directory holding
rem `.lyracore-cli-rev`), builds the Windows CLI vendored under `tools\lyracore-cli` on first use,
rem and runs it with YOUR current directory preserved — the CLI discovers the workspace from the
rem working directory, so it must not be changed out from under it.
rem
rem The vendored CLI is a fork of LyraCoreProject/lyracore-cli with the Unix-only process and
rem console calls replaced by Win32 equivalents (see `tools\lyracore-cli\src\platform.rs`). It is a
rem normal Cargo package: `cargo build --manifest-path tools\lyracore-cli\Cargo.toml` rebuilds it
rem after an edit, and this launcher rebuilds automatically only when the binary is missing.
setlocal enabledelayedexpansion

set "dir=%CD%"
:walk
if exist "%dir%\.lyracore-cli-rev" goto found
for %%I in ("%dir%\..") do set "parent=%%~fI"
if /I "!parent!"=="!dir!" goto notfound
set "dir=!parent!"
goto walk

:notfound
>&2 echo lyracore: not inside a LyraCore checkout ^(no .lyracore-cli-rev here or in any parent of %CD%^)
exit /b 2

:found
set "cli=%dir%\tools\lyracore-cli"
if not exist "%cli%\Cargo.toml" (
    >&2 echo lyracore: %cli%\Cargo.toml is missing — this checkout has no vendored Windows CLI.
    exit /b 2
)
if not exist "%cli%\target\debug\lyracore.exe" (
    >&2 echo lyracore: preparing the Windows CLI ^(first run — this takes a moment^)...
)
rem Cargo's own freshness check makes this a no-op after the first build, so an edited vendored CLI
rem is always what runs. The binary is then exec'd directly, preserving the caller's working
rem directory — the CLI discovers the workspace from it.
cargo build --quiet --manifest-path "%cli%\Cargo.toml"
if errorlevel 1 exit /b 1
"%cli%\target\debug\lyracore.exe" %*
exit /b %ERRORLEVEL%
