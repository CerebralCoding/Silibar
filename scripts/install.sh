#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  echo "Usage: ./scripts/install.sh [layout-name]"
  echo "Build and install Silibar. The default layout name is silibar."
  echo "Override SILIBAR_METRICS_URL, ZELLIJ_LAYOUT_DIR, or ZELLIJ_PLUGIN_DIR as needed."
  exit 0
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
layout_name="${1:-silibar}"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/zellij"
layout_dir="${ZELLIJ_LAYOUT_DIR:-$config_dir/layouts}"
plugin_dir="${ZELLIJ_PLUGIN_DIR:-$config_dir/plugins}"
wasm_path="$repo_root/target/wasm32-wasip1/release/silibar.wasm"
metrics_url="${SILIBAR_METRICS_URL:-http://127.0.0.1:9090/json}"

if [[ $# -gt 1 || ! "$layout_name" =~ ^[a-zA-Z0-9_-]+$ ]]; then
  echo "Use one layout name containing only letters, numbers, underscores, or hyphens." >&2
  exit 1
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Silibar is intended for macOS on Apple Silicon." >&2
  exit 1
fi

if [[ "$(uname -m)" != "arm64" ]]; then
  echo "Silibar requires an Apple Silicon Mac (arm64)." >&2
  exit 1
fi

for command in cargo rustup zellij macmon; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Missing required command: $command" >&2
    exit 1
  fi
done

if ! macmon serve --help | grep -q -- "--host"; then
  echo "The installed macmon does not support 'serve --host'." >&2
  echo "Install the current upstream release with: brew install macmon" >&2
  exit 1
fi

rustup target add wasm32-wasip1

cd "$repo_root"
cargo build --locked --target-dir "$repo_root/target" --target wasm32-wasip1 --release --bin silibar

wasm_hash="$(shasum -a 256 "$wasm_path")"
wasm_hash="${wasm_hash%% *}"
mkdir -p "$layout_dir" "$plugin_dir"
layout_dir="$(cd "$layout_dir" && pwd)"
layout_path="$layout_dir/$layout_name.kdl"
plugin_dir="$(cd "$plugin_dir" && pwd)"
installed_wasm_path="$plugin_dir/silibar-${wasm_hash:0:16}.wasm"
cp "$wasm_path" "$installed_wasm_path"

kdl_wasm_path="${installed_wasm_path//\\/\\\\}"
kdl_wasm_path="${kdl_wasm_path//\"/\\\"}"
kdl_metrics_url="${metrics_url//\\/\\\\}"
kdl_metrics_url="${kdl_metrics_url//\"/\\\"}"
cat >"$layout_path" <<KDL
layout {
    default_tab_template {
        pane size=1 borderless=true {
            plugin location="file:$kdl_wasm_path" {
                metrics_url "$kdl_metrics_url"
            }
        }

        children

        pane size=1 borderless=true {
            plugin location="zellij:status-bar"
        }
    }

    tab {
        pane
    }
}
KDL

cat <<EOF
Installed Silibar.

Layout written to:
  $layout_path

Content-addressed plugin installed at:
  $installed_wasm_path

Launch with:
  zellij --layout "$layout_path"

To make it the default for new Zellij sessions, add this to:
  $config_dir/config.kdl

  default_layout "$layout_name"

Existing sessions keep their old layout; start a new session to use Silibar.

When Zellij asks for plugin permissions, allow WebAccess,
ReadApplicationState, and ChangeApplicationState.
EOF
