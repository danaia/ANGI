#!/bin/bash
# Global ANGI swarm command; agents and credentials stay under the caller's UID.
set -euo pipefail
agent_workdir=$PWD
agent_machine=$(uname -m)
if [[ $agent_machine == x86_64 && ${HOME:-} == /* && -d ${HOME%/}/.angi-agent ]]; then
    agent_store=${HOME%/}/.angi-agent
elif mountpoint -q /var/lib/agx-data; then
    agent_store=/var/lib/agx-data/agx-opencode-users/$(id -u)
elif [[ $agent_machine == x86_64 && ${HOME:-} == /* ]]; then
    agent_store=${HOME%/}/.angi-agent
else
    echo 'ANGI agents need the NVMe data volume.' >&2
    exit 1
fi
umask 077
mkdir -m 700 -- "$agent_store" 2>/dev/null || true
cd -P -- "$agent_store"
[[ -O . && $(pwd -P) == "$agent_store" ]] || { echo 'Unsafe ANGI agent storage.' >&2; exit 1; }
chmod 700 .
mkdir -p data cache state config tmp
export XDG_DATA_HOME=$agent_store/data XDG_CACHE_HOME=$agent_store/cache
export XDG_STATE_HOME=$agent_store/state XDG_CONFIG_HOME=$agent_store/config
export TMPDIR=$agent_store/tmp OPENCODE_DISABLE_AUTOUPDATE=true
export npm_config_cache=$agent_store/cache/npm BUN_INSTALL_CACHE_DIR=$agent_store/cache/bun
agent_template=$(readlink -f /usr/share/git-core/templates/info/exclude)
export GIT_TEMPLATE_DIR=${agent_template%/info/exclude}
export HERDR_CONFIG_PATH=$agent_store/config/herdr/config.toml
cd -- "$agent_workdir"
exec /usr/bin/python3 /usr/libexec/agx-agent/main.py "$@"
