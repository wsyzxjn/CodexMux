#!/bin/zsh
set -euo pipefail

script_dir="${0:A:h}"
configuration="${1:-release}"
architecture="${2:-}"
cli_binary="${3:-}"
app_dir="$script_dir/.build/$configuration/CodexMux.app"
contents_dir="$app_dir/Contents"

# Resolve an explicitly supplied path before changing into the Swift package.
if [[ -n "$cli_binary" && "$cli_binary" != /* ]]; then
  cli_binary="$PWD/$cli_binary"
fi

cd "$script_dir"
build_arguments=(build -c "$configuration")
if [[ -n "$architecture" ]]; then
  build_arguments+=(--arch "$architecture")
fi
swift "${build_arguments[@]}"
binary_dir="$(swift "${build_arguments[@]}" --show-bin-path)"

if [[ -z "$cli_binary" ]]; then
  if [[ "$architecture" == "arm64" ]]; then
    cli_binary="$script_dir/../target/aarch64-apple-darwin/release/codexmux"
  else
    cli_binary="$script_dir/../target/release/codexmux"
  fi
fi
if [[ ! -x "$cli_binary" ]]; then
  print -u2 -- "codexmux CLI binary is missing or not executable: $cli_binary"
  print -u2 -- "build it first with cargo build --release (or pass it as the third argument)"
  exit 1
fi

mkdir -p "$contents_dir/MacOS" "$contents_dir/Resources"
install -m 755 "$binary_dir/CodexMux" "$contents_dir/MacOS/CodexMux"
install -m 755 "$cli_binary" "$contents_dir/Resources/codexmux"
install -m 644 "Resources/Info.plist" "$contents_dir/Info.plist"
install -m 644 "Resources/CodexMux.icns" "$contents_dir/Resources/CodexMux.icns"

codesign --force --sign - --timestamp=none "$contents_dir/Resources/codexmux"
codesign --force --sign - --timestamp=none "$app_dir"
print -r -- "$app_dir"
