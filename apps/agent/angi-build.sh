#!/bin/bash
set -euo pipefail
# A build must never starve the desktop it runs beside: run at the lowest priority and leave one core free.
if [[ -z ${ANGI_BUILD_NICE:-} ]]; then
    export ANGI_BUILD_NICE=1
    exec nice -n 19 "$0" "$@"
fi
source /usr/share/agx/agent/sdk-env.sh
cores=$(nproc 2>/dev/null || echo 2)
export CARGO_BUILD_JOBS=$(( cores > 1 ? cores - 1 : 1 ))
umask 077
build_store=${CARGO_HOME%/cargo}
mkdir -m700 -- "$build_store" 2>/dev/null || true
[[ -d $build_store && -O $build_store && ! -L $build_store ]] || { echo 'Unsafe ANGI build storage.' >&2; exit 1; }
chmod 700 "$build_store"
cd /var/lib/agx-data/angi-dev
# The desktop's Slint UI compiles into 8 MB of generated Rust. Without incremental compilation every
# change recompiles all of it (about 50 s on a 16-core workstation, about 1 minute here); with it, only
# what changed (about 5 s on the workstation). The first build after this is a full one.
export CARGO_INCREMENTAL=1
# Dependencies are only cached for one set of manifests and one compiler. When either has changed since the
# last good desktop build here (for example a release built on the workstation added a feature), this build
# recompiles the dependency graph. Say so, because it takes minutes instead of seconds.
warm_stamp=$CARGO_TARGET_DIR/release/.angi-gui-deps
warm_key=
if [[ ${1:-} == gui ]]; then
    warm_key=$( { cat Cargo.lock crates/agx-gui/Cargo.lock crates/agx-gui/Cargo.toml; rustc -V; } 2>/dev/null | sha256sum | cut -c1-16)
    if [[ $(cat "$warm_stamp" 2>/dev/null || true) != "$warm_key" ]]; then
        echo 'angi-build: the dependencies changed since the last desktop build on this device, so this one compiles them again (about 14 minutes at low priority). Later builds of a small change take about a minute.' >&2
    fi
fi
case "${1:-}" in
    cli) cargo build --locked --release -p agx-cli ;;
    gui) cargo build --locked --release --manifest-path crates/agx-gui/Cargo.toml --no-default-features --features kms,wayland
         [[ -n $warm_key ]] && printf '%s\n' "$warm_key" > "$warm_stamp" ;;
    test) cargo test --locked --workspace -- --test-threads=1 ;;
    gui-test) cargo test --locked --manifest-path crates/agx-gui/Cargo.toml --no-default-features --features kms,wayland ;;
    env) echo /usr/share/agx/agent/sdk-env.sh ;;
    *) echo 'Usage: angi-build {cli|gui|test|gui-test|env}' >&2; exit 2 ;;
esac
