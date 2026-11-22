#!/bin/sh
# A bare launch starts Radio Paradise with the channel list and worldwide Radio provider.
if [ "$#" -eq 0 ]; then
    set -- --provider radio --auto-play /usr/share/agx/sound/stations.m3u
fi
exec /usr/libexec/agx-sound/cliamp "$@"
