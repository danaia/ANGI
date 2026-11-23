#!/bin/bash
set -euo pipefail
source /usr/share/agx/agent/sdk-env.sh
umask 077
build_store=${CARGO_HOME%/cargo}
mkdir -m700 -- "$build_store" 2>/dev/null || true
[[ -d $build_store && -O $build_store && ! -L $build_store ]] || { echo 'Unsafe ANGI build storage.' >&2; exit 1; }
chmod 700 "$build_store"
cd /var/lib/agx-data/angi-dev
# The desktop's Slint UI compiles into 8 MB of generated Rust. Without incremental compilation every
# change recompiles all of it (about 50 s on a 16-core workstation, 2 minutes here); with it, only
# what changed (about 5 s on the workstation). The first build after this is a full one.
export CARGO_INCREMENTAL=1
case "${1:-}" in
    cli) cargo build --locked --release -p agx-cli ;;
    gui) cargo build --locked --release --manifest-path crates/agx-gui/Cargo.toml --no-default-features --features kms,wayland ;;
    test) cargo test --locked --workspace -- --test-threads=1 ;;
    gui-test) cargo test --locked --manifest-path crates/agx-gui/Cargo.toml --no-default-features --features kms,wayland ;;
    env) echo /usr/share/agx/agent/sdk-env.sh ;;
    *) echo 'Usage: angi-build {cli|gui|test|gui-test|env}' >&2; exit 2 ;;
esac
