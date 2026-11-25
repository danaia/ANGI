#!/usr/bin/env bash
# Build the personal and public images for one board on the build machine, check both and copy
# them to ~/angi-build/out/{personal,public}. Run it detached (nohup ... &) and read its log.
#   BOARD=<registry id> ~/angi-build/remote-build.sh
set -euo pipefail
BOARD=${BOARD:?set BOARD to a registry id from scripts/boards.json}
SRC=${SRC:-$HOME/angi-build/src}
cd "$SRC"
export AGX_BUILD_OUTPUT=$SRC/output-$BOARD
OUT=$AGX_BUILD_OUTPUT
export JOBS=${JOBS:-16}
mkdir -p ~/angi-build/out/personal ~/angi-build/out/public

# Personal build: the workstation's public key is baked in, for development.
unset AGX_GENERIC_ISO; rm -f "$OUT/.generic-iso"
export SSH_PUBLIC_KEY=${SSH_PUBLIC_KEY:-$HOME/angi-build/workstation.pub}
make configure BOARD="$BOARD"
make dependencies BOARD="$BOARD"
make build BOARD="$BOARD"
make check-image BOARD="$BOARD"
cp -f "$OUT/images/disk.img" "$OUT/images/disk.img.sha256" ~/angi-build/out/personal/
echo STAGE=personal-done

# Public build: no keys, root locked, set up on screen. This is the one that may be shared.
export AGX_GENERIC_ISO=1
make build BOARD="$BOARD"
make check-image BOARD="$BOARD"
cp -f "$OUT/images/disk.img" "$OUT/images/disk.img.sha256" ~/angi-build/out/public/
echo STAGE=public-done
echo ALLDONE
