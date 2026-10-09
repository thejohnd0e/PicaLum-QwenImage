#!/usr/bin/env bash
# Build PicaLum QwenImage from Git Bash, Warp, MSYS2 or WSL (Windows only: it builds a Windows app).
# Usage: ./build.sh            app + CLI into ./out
#        ./build.sh -Installer also the NSIS installer
set -euo pipefail
cd "$(dirname "$0")"
if command -v pwsh >/dev/null 2>&1; then PS=pwsh
elif command -v powershell.exe >/dev/null 2>&1; then PS=powershell.exe
else echo "PowerShell not found. This builds a Windows app: run it on Windows (Git Bash, Warp, CMD or PowerShell)." >&2; exit 1; fi
# From WSL, hand PowerShell a Windows path.
SCRIPT="build.ps1"
if command -v wslpath >/dev/null 2>&1; then SCRIPT="$(wslpath -w "$PWD/build.ps1")"; fi
exec "$PS" -NoProfile -ExecutionPolicy Bypass -File "$SCRIPT" "$@"
