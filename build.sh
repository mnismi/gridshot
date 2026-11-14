#!/usr/bin/env bash
# Build Gridshot from WSL with the Windows Rust toolchain (cargo.exe + MSVC).
# On Windows itself just use `cargo build --release`.
#
#   ./build.sh            release build, copied to dist/gridshot.exe
#   ./build.sh test       run the unit tests
#   ./build.sh shots      release build, then save screenshots of every screen to docs/screenshots
#
# Build output goes to %LOCALAPPDATA%\gridshot-target (building inside the WSL file
# system from Windows is slow). Set GRIDSHOT_TARGET_DIR to a Windows path to change it.
set -euo pipefail
cd "$(dirname "$0")"
here=$(wslpath -w "$PWD")
target=${GRIDSHOT_TARGET_DIR:-$(powershell.exe -NoProfile -Command '[Environment]::GetFolderPath("LocalApplicationData")' | tr -d '\r')\\gridshot-target}

cargo() {
  powershell.exe -NoProfile -Command "\$ErrorActionPreference='Continue'; cd '$here'; \$env:CARGO_TARGET_DIR='$target'; cargo $* 2>&1 | ForEach-Object { \"\$_\" }; exit \$LASTEXITCODE"
}

release() {
  cargo build --release
  mkdir -p dist
  cp "$(wslpath -u "$target")/release/gridshot.exe" dist/gridshot.exe
  ls -l dist/gridshot.exe
}

case ${1:-release} in
  test) cargo test ;;
  release) release ;;
  shots)
    release
    exe="$target\\release\\gridshot.exe"
    powershell.exe -NoProfile -Command "cd '$here'; \$env:GRIDSHOT_DEMO='shots'; \$p = Start-Process -FilePath '$exe' -PassThru -Wait; exit \$p.ExitCode"
    ls -l docs/screenshots
    ;;
  *) echo "usage: $0 [release|test|shots]" >&2; exit 2 ;;
esac
