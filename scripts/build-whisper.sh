#!/usr/bin/env bash
# Cross-build whisper.cpp's command-line transcriber for the ANGI device with the pinned Buildroot
# toolchain. Output: .cache/whisper/whisper-cli (static, no OpenMP), installed by deploy-agent.sh.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
TAG=v1.9.6
COMMIT=96ec4d9823e9d0211944172da35f9f29134af990
BOARD=${BOARD:-rpi5}
HOST="$ROOT/output/host"
[[ -x "$HOST/bin/cmake" && -f "$HOST/share/buildroot/toolchainfile.cmake" ]] || {
    echo 'Build the Buildroot host tools first (output/host).' >&2; exit 2; }
WORK="$ROOT/.cache/whisper"
SOURCE="$WORK/src"
mkdir -p "$WORK"
if [[ ! -d "$SOURCE/.git" ]]; then
    git clone --quiet --depth 1 --branch "$TAG" https://github.com/ggml-org/whisper.cpp "$SOURCE"
fi
[[ $(git -C "$SOURCE" rev-parse HEAD) == "$COMMIT" ]] || { echo "whisper.cpp is not at the pinned commit." >&2; exit 1; }
# The Pi 5's Cortex-A76 has dot-product and half-precision maths; earlier boards do not.
case "$BOARD" in
    rpi5) ARCH=armv8.2-a+dotprod+fp16 ;;
    *)    ARCH=armv8-a ;;
esac
"$HOST/bin/cmake" -S "$SOURCE" -B "$WORK/build-$BOARD" \
    -DCMAKE_TOOLCHAIN_FILE="$HOST/share/buildroot/toolchainfile.cmake" \
    -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF \
    -DGGML_NATIVE=OFF -DGGML_CPU_ARM_ARCH="$ARCH" -DGGML_OPENMP=OFF \
    -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_SERVER=OFF -DWHISPER_BUILD_EXAMPLES=ON \
    -DWHISPER_CURL=OFF -DWHISPER_SDL2=OFF
"$HOST/bin/cmake" --build "$WORK/build-$BOARD" --target whisper-cli -j"${WHISPER_BUILD_JOBS:-4}"
install -m755 "$WORK/build-$BOARD/bin/whisper-cli" "$WORK/whisper-cli-$BOARD"
"$HOST/bin/aarch64-linux-strip" "$WORK/whisper-cli-$BOARD" 2>/dev/null || true
echo "Built $WORK/whisper-cli-$BOARD"
