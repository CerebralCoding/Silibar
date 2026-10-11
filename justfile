[group("build")]
build-wasm:
    cargo build --locked --target wasm32-wasip1 --release --bin silibar

[group("run")]
run-debug:
    cargo build --locked --target wasm32-wasip1 --bin silibar
    zellij action start-or-reload-plugin file:target/wasm32-wasip1/debug/silibar.wasm

[group("run")]
run: build-wasm
    zellij action start-or-reload-plugin file:target/wasm32-wasip1/release/silibar.wasm

[group("install")]
install:
    ./scripts/install.sh

[group("test")]
test:
    cargo test --locked

[group("test")]
check:
    cargo fmt --all -- --check
    cargo clippy --locked --all-targets -- -D warnings
    cargo test --locked
    bash -n scripts/install.sh
