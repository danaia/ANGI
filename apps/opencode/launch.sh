#!/bin/bash
# Run the pinned CLI as the caller, with private data and caches.
set -euo pipefail
opencode_workdir=$PWD
opencode_machine=$(uname -m)
if [[ $opencode_machine == x86_64 && ${HOME:-} == /* && -d ${HOME%/}/.angi-agent ]]; then
    opencode_store=${HOME%/}/.angi-agent
elif mountpoint -q /var/lib/agx-data; then
    opencode_store=/var/lib/agx-data/agx-opencode-users/$(id -u)
elif [[ $opencode_machine == x86_64 && ${HOME:-} == /* ]]; then
    opencode_store=${HOME%/}/.angi-agent
else
    printf 'OpenCode needs the ANGI NVMe data volume to be mounted.\n' >&2
    exit 1
fi
umask 077
mkdir -m 700 -- "$opencode_store" 2>/dev/null || true
# Pin the actual directory as cwd before checking it: an untrusted user can
# reserve a UID name in the sticky parent, but cannot redirect this process.
cd -P -- "$opencode_store"
if [[ ! -O . || $(pwd -P) != "$opencode_store" ]]; then
    printf 'OpenCode storage must be a private directory owned by your account.\n' >&2
    exit 1
fi
chmod 700 .
mkdir -p data cache state config tmp
export XDG_DATA_HOME=$opencode_store/data
export XDG_CACHE_HOME=$opencode_store/cache
export XDG_STATE_HOME=$opencode_store/state
export XDG_CONFIG_HOME=$opencode_store/config
export TMPDIR=$opencode_store/tmp
export npm_config_cache=$opencode_store/cache/npm
export BUN_INSTALL_CACHE_DIR=$opencode_store/cache/bun
# Deployment exposes individual runtime files through symlinks. Git must copy
# the underlying regular templates so OpenCode can update its snapshot exclude.
opencode_template=$(readlink -f /usr/share/git-core/templates/info/exclude)
export GIT_TEMPLATE_DIR=${opencode_template%/info/exclude}
# The binary is root-owned and versioned. Updates go through ANGI deployment.
export OPENCODE_DISABLE_AUTOUPDATE=true
cd -- "$opencode_workdir"
exec /usr/libexec/opencode/opencode "$@"
