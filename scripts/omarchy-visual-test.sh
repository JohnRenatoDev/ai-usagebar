#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output_dir=${1:-"$repo_root/target/omarchy-visual-scenarios"}
shell_root=${OMARCHY_SHELL_DIR:-"${XDG_DATA_HOME:-$HOME/.local/share}/omarchy/shell"}

if ! command -v quickshell >/dev/null 2>&1; then
  echo "quickshell is required for Omarchy visual scenarios" >&2
  exit 1
fi
for module in Commons Ui; do
  if [[ ! -d "$shell_root/$module" ]]; then
    echo "Omarchy shell module not found: $shell_root/$module" >&2
    echo "Set OMARCHY_SHELL_DIR to the shell source/install directory." >&2
    exit 1
  fi
done

mkdir -p -- "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/ai-usagebar-visual.XXXXXX")
cleanup() {
  [[ -n ${scratch:-} && -d $scratch ]] && rm -rf -- "$scratch"
}
trap cleanup EXIT
capture_dir="$scratch/output"
mkdir -- "$capture_dir"

ln -s -- "$shell_root/Commons" "$scratch/Commons"
ln -s -- "$shell_root/Ui" "$scratch/Ui"
ln -s -- "$repo_root/omarchy" "$scratch/Usage"
ln -s -- "$repo_root/omarchy/visual-tests/shell.qml" "$scratch/shell.qml"

AI_USAGEBAR_VISUAL_OUTPUT_DIR="$capture_dir" \
QT_QPA_PLATFORM=offscreen \
timeout 15 quickshell -p "$scratch" --no-color \
  --log-rules 'qt.qml.connections=false'

for image in settings-display-dark.png settings-credentials-light.png \
  notices-dark.png notices-light-pt-BR.png settings-priorities-dark.png \
  priorities-dark.png priorities-light-pt-BR.png; do
  if [[ ! -s "$capture_dir/$image" ]]; then
    echo "Visual scenario did not produce $image" >&2
    exit 1
  fi
  install -m 0644 -- "$capture_dir/$image" "$output_dir/$image"
done

echo "Omarchy visual scenarios: $output_dir"
