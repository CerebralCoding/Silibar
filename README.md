# Silibar

An Apple Silicon metrics and tab bar for [Zellij](https://zellij.dev), powered by
[macmon](https://github.com/vladkens/macmon). CPU, GPU, memory, power, and CPU
temperature sit alongside clickable native Zellij tab ribbons.

```text
 Zellij (work)  Tab #1                         CPU:  42% | GPU:  73% | RAM:  8.0/16G | PWR: 12.3W | TEMP: 55C
```

At 120 columns or fewer, labels disappear while the metric order stays the same:

```text
 42% |  73% |  8.0/16G | 12.3W | 55C
```

Silibar polls macmon's local HTTP server once per second. Sampling runs outside
Zellij, so sessions share one macmon process. It requires no administrator access.

## Install

Requires an Apple Silicon Mac, Zellij 0.45.x, macmon 0.9.0 or newer with
`serve --host`, and a current stable [Rust toolchain](https://rustup.rs).

### With an agent

Give any coding agent with terminal access to your Mac this prompt:

> Install Silibar on this Mac by following https://github.com/CerebralCoding/Silibar/blob/main/AGENTS.md. Complete the setup and verify it, preserving my existing Zellij configuration.

The [agent installation guide](AGENTS.md) covers dependencies, installation, and
verification. You will need to approve Zellij's plugin permissions on first launch.

### Manually

Install the runtime dependencies and start macmon as a localhost-only LaunchAgent:

```bash
brew install zellij macmon
macmon serve --host 127.0.0.1 --port 9090 --install
```

From this repository, build and install the plugin and layout:

```bash
./scripts/install.sh
zellij --layout silibar
```

The installer adds Rust's `wasm32-wasip1` target, builds the plugin, and writes
`~/.config/zellij/layouts/silibar.kdl` and a content-addressed WASM file in
`~/.config/zellij/plugins/`. It respects `XDG_CONFIG_HOME`. Reinstalling updates
the layout; the content hash prevents Zellij from reusing an older cached build.

Allow these permissions when Zellij prompts:

- `WebAccess` to read macmon's `/json` endpoint.
- `ReadApplicationState` to display sessions, tabs, and input modes.
- `ChangeApplicationState` to switch tabs and exit when the last selectable pane closes.

To use Silibar for new sessions, add `default_layout "silibar"` to your Zellij
`config.kdl`. Existing sessions keep their current layout.

## Configuration

Use a custom layout name or metrics endpoint during installation:

```bash
SILIBAR_METRICS_URL=http://127.0.0.1:9091/json ./scripts/install.sh my-status
```

`ZELLIJ_LAYOUT_DIR` and `ZELLIJ_PLUGIN_DIR` override the installation directories.
The installer prints an absolute launch command for custom locations.

For an existing layout, add a one-line pane using the installed WASM path:

```kdl
pane size=1 borderless=true {
    plugin location="file:/absolute/path/to/silibar.wasm" {
        metrics_url "http://127.0.0.1:9090/json"
    }
}
```

Set `render_mode "metrics"` inside the plugin block to display only metrics.

## Development

```bash
rustup target add wasm32-wasip1
cargo build --locked --release --target wasm32-wasip1 --bin silibar
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

The build produces `target/wasm32-wasip1/release/silibar.wasm`.
[Just](https://github.com/casey/just) is optional: `just build-wasm`, `just check`,
`just install`, and `just run-debug` wrap the common tasks.

## License

[MIT](LICENSE), copyright © 2026 Sebastian Christiansen.
Zellij and macmon are independent projects with their own licenses.
