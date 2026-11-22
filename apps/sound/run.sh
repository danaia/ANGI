#!/bin/sh
# A single unprivileged PulseAudio server owns audio routing for ANGI.
set -eu
modprobe snd-usb-audio 2>/dev/null || :
modprobe snd-bcm2835 2>/dev/null || :

runtime=/run/pulse
state=/var/lib/pulse
if mountpoint -q /var/lib/agx-data; then
    state=/var/lib/agx-data/sound
fi
install -d -m 0700 -o pulse -g pulse "$state"
install -d -m 0755 -o pulse -g pulse "$runtime"
export PULSE_STATE_PATH="$state"
control=/run/agx-sound
install -d -m 0750 -o root -g pulse "$control"
export PULSE_COOKIE="$control/control-cookie"
if [ ! -s "$PULSE_COOKIE" ]; then
    umask 077
    head -c 256 /dev/urandom > "$PULSE_COOKIE"
    chgrp pulse "$PULSE_COOKIE"
    chmod 640 "$PULSE_COOKIE"
    umask 022
fi
export PULSE_SERVER="unix:$runtime/native"

pulseaudio --system --daemonize=no --disallow-exit --exit-idle-time=-1 --disable-shm \
    -n -F /etc/pulse/agx-system.pa &
server=$!
trap 'kill "$server" 2>/dev/null || :; wait "$server" 2>/dev/null || :' EXIT
trap 'exit 0' INT TERM
# PulseAudio's udev module requires a running udev daemon. ANGI uses devtmpfs,
# so reconcile wired PCM devices against ALSA's kernel inventory instead.
modules="$control/pcms"
install -d -m 0700 "$modules"
rm -f "$modules"/pcmC*

inode_of() {
    ls -i "$1" 2>/dev/null | { read -r inode rest && printf '%s' "$inode"; }
}

# The ALSA period wakeup timer is opened when PulseAudio configures a PCM.
if [ -c /dev/snd/timer ]; then
    chgrp pulse /dev/snd/timer
    chmod 660 /dev/snd/timer
fi

# The vc4 HDMI PCM remains in devtmpfs when the cable is unplugged.
hdmi_connected() {
    for status in /sys/class/drm/card*-HDMI-A-"$1"/status; do
        [ ! -r "$status" ] && continue
        read -r connection < "$status" || continue
        [ "$connection" = connected ] && return 0
    done
    return 1
}

endpoint_connected() {
    case "$1" in
        alsa_output.vc4hdmi0.*) hdmi_connected 1 ;;
        alsa_output.vc4hdmi1.*) hdmi_connected 2 ;;
        *) return 0 ;;
    esac
}

bus_inode=''
while kill -0 "$server" 2>/dev/null; do
    if [ ! -S "$runtime/native" ]; then
        sleep 1
        continue
    fi
    if [ -S /run/dbus/system_bus_socket ]; then
        current_bus=$(inode_of /run/dbus/system_bus_socket || :)
        if [ "$current_bus" != "$bus_inode" ]; then
            if [ -n "$bus_inode" ]; then
                pactl unload-module module-bluetooth-discover 2>/dev/null || :
            fi
            if pactl load-module module-bluetooth-discover headset=native \
                enable_native_hsp_hs=true >/dev/null 2>&1; then
                bus_inode=$current_bus
            fi
        fi
    fi
    for record in "$modules"/*; do
        [ -f "$record" ] || continue
        IFS=' ' read -r inode module endpoint < "$record"
        node="/dev/snd/${record##*/}"
        current=$(inode_of "$node" || :)
        if [ "$inode" != "$current" ] || ! endpoint_connected "$endpoint"; then
            pactl unload-module "$module" 2>/dev/null || :
            rm -f "$record"
        fi
    done
    for node in /dev/snd/pcmC*D*[pc]; do
        [ -c "$node" ] || continue
        name=${node##*/}
        record="$modules/$name"
        [ ! -f "$record" ] || continue
        card=${name#pcmC}
        card=${card%D*}
        device=${name#*D}
        device=${device%?}
        read -r id < "/proc/asound/card$card/id" || continue
        case "$id" in ""|*[!a-zA-Z0-9_]*) continue ;; esac
        mixer="/dev/snd/controlC$card"
        [ -c "$mixer" ] || continue
        chgrp pulse "$node" "$mixer" 2>/dev/null || continue
        chmod 660 "$node" "$mixer" 2>/dev/null || continue
        inode=$(inode_of "$node") || continue
        case "$name" in
            *p) kind=sink; direction=output ;;
            *c) kind=source; direction=input ;;
        esac
        endpoint="alsa_${direction}.$id.$device"
        endpoint_connected "$endpoint" || continue
        label=''
        plug=hw
        case "$id:$direction" in
            vc4hdmi0:output) plug=hdmi; label='HDMI 1' ;;
            vc4hdmi1:output) plug=hdmi; label='HDMI 2' ;;
            Headphones:output) label='3.5 mm headphones' ;;
        esac
        set -- "device=$plug:CARD=$id,DEV=$device" "${kind}_name=$endpoint" tsched=0
        if [ -n "$label" ]; then
            set -- "$@" "${kind}_properties='device.description=\"$label\"'"
        fi
        if module=$(pactl load-module "module-alsa-$kind" "$@" 2>/dev/null); then
            printf '%s %s %s\n' "$inode" "$module" "$endpoint" > "$record"
        fi
    done
    sleep 2
done
exit 1
