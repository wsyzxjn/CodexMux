#!/bin/zsh
set -euo pipefail

script_dir="${0:A:h}"
configuration="${1:-release}"
architecture="${2:-}"
app_dir="$script_dir/.build/$configuration/CodexMux.app"
contents_dir="$app_dir/Contents"

cd "$script_dir"
build_arguments=(build -c "$configuration")
if [[ -n "$architecture" ]]; then
  build_arguments+=(--arch "$architecture")
fi
swift "${build_arguments[@]}"
binary_dir="$(swift "${build_arguments[@]}" --show-bin-path)"

mkdir -p "$contents_dir/MacOS" "$contents_dir/Resources"
install -m 755 "$binary_dir/CodexMux" "$contents_dir/MacOS/CodexMux"
install -m 644 "Resources/Info.plist" "$contents_dir/Info.plist"
install -m 644 "Resources/CodexMux.icns" "$contents_dir/Resources/CodexMux.icns"

codesign --force --sign - --timestamp=none "$app_dir"
print -r -- "$app_dir"
