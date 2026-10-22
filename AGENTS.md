# Agent installation guide

Follow this workflow when a user asks you to install Silibar. Complete the setup
on their Mac; do not merely repeat these instructions. For code changes, use the
development commands in [README.md](README.md#development).

## 1. Check prerequisites

Run `uname -s` and `uname -m`: this requires `Darwin` and `arm64`. On an Intel Mac
or another operating system, stop and explain that Silibar is unsupported. If
the shell is running under Rosetta, use a native Apple Silicon shell.

Check `zellij --version`, `macmon --version`, `macmon serve --help`,
`rustup --version`, and `cargo --version` against the [requirements](README.md#install).
Reuse compatible installations. Install missing runtime tools with an existing
Homebrew installation:

```bash
brew install zellij macmon
```

If Rust is missing, use the official [rustup installer](https://rustup.rs) with
the stable toolchain and minimal profile. Ensure `rustup` and `cargo` are on the
agent's PATH. Preserve any existing Rust default toolchain.

Never run `sudo` or another administrator execution mechanism, including through
an installer. If Homebrew or Apple's command-line tools are missing and setup
requires administrator access or a user dialog, report that prerequisite for
the user to install manually, then resume when it is available.

## 2. Get the source

Reuse an existing Silibar checkout without discarding local changes. Otherwise,
clone into the user's normal projects directory:

```bash
git clone git@github.com:CerebralCoding/Silibar.git
```

If GitHub SSH access is unavailable, download and extract the
[public source archive](https://github.com/CerebralCoding/Silibar/archive/refs/heads/main.tar.gz)
there instead. Run the remaining installation commands from the repository root.

## 3. Start metrics

Reuse an existing macmon service bound to localhost. Otherwise, install its
user LaunchAgent, which starts at login:

```bash
macmon serve --host 127.0.0.1 --port 9090 --install
```

Verify that the endpoint returns metrics JSON, allowing a moment for startup:

```bash
curl --fail --silent --show-error --max-time 5 http://127.0.0.1:9090/json
```

If port 9090 is occupied, identify the listener instead of stopping an unrelated
process. Use a free localhost port for macmon and set `SILIBAR_METRICS_URL` to its
`/json` URL when running the installer.

## 4. Install the plugin and layout

Run `zellij setup --check` to locate the user's configuration. The installer
uses `${XDG_CONFIG_HOME:-$HOME/.config}/zellij`; for a custom location, set
`ZELLIJ_LAYOUT_DIR` and `ZELLIJ_PLUGIN_DIR` as described in the
[configuration instructions](README.md#configuration).

Back up an existing `silibar.kdl` before replacing it, or pass a different layout
name to the installer. Preserve other layouts, plugins, and configuration.

```bash
./scripts/install.sh
```

This adds the WASM target, builds with the locked dependencies, installs the
layout and content-addressed plugin, and pre-approves `WebAccess`,
`ReadApplicationState`, and `ChangeApplicationState` for that build. Other
plugins' permissions are preserved. If Zellij uses a custom permission cache,
set `ZELLIJ_PERMISSION_CACHE` to its `permissions.kdl` path before installation.
Use the installer's printed paths and launch command.
Only change `default_layout` if the user requested it; update that setting in
the existing `config.kdl` without replacing the file or adding a duplicate key.

## 5. Verify and hand off

Confirm that the printed layout and WASM files exist and are nonempty, that the
layout references the installed WASM and correct metrics URL, and that the
printed permission cache grants the three permissions to the installed plugin.
Check that the metrics endpoint still returns JSON. Run `zellij setup --check`
again if you changed the configuration.

Launch the printed command in an interactive terminal outside an existing Zellij
session. Do not kill or restart the user's sessions; existing sessions keep
their old layouts. No permission prompt is expected for this build. If one
appears, check that the layout's plugin path and Zellij's permission cache match
the installer's output, then rerun the installer with the correct paths.

Confirm that tabs and updating CPU/GPU usage and temperatures, ANE power, memory,
and system power appear. If an interactive launch is unavailable, report the
completed file and endpoint checks and give the user the exact launch command;
do not claim that the UI was verified. Finish with the installed paths, whether
the default layout changed, and any remaining user action.
