#!/bin/sh
# Install the cursor data and aliases from GNOME's Adwaita theme.
set -eu
source_dir=$1
icons=$2/usr/share/icons
licenses=$2/usr/share/licenses/adwaita-cursors

mkdir -p "$icons/Adwaita/cursors" "$icons/default" "$licenses"
cp "$source_dir"/Adwaita/cursors/* "$icons/Adwaita/cursors/"
cp "$source_dir"/COPYING* "$licenses/"

# Qt and Wayland clients can request either CSS names or the legacy aliases.
(
	cd "$icons/Adwaita/cursors"
	while read -r cursor aliases; do
		for alias in $aliases; do
			ln -sf "$cursor" "$alias"
		done
	done <<'ALIASES'
all-resize fleur
crosshair cross cross_reverse diamond_cross tcross
default arrow dnd-move left_ptr top_left_arrow move
e-resize right_side
ew-resize sb_h_double_arrow
grab hand1
help question_arrow
n-resize top_side
ne-resize top_right_corner
nesw-resize fd_double_arrow
ns-resize sb_v_double_arrow
nw-resize top_left_corner
nwse-resize bd_double_arrow
pointer hand2
s-resize bottom_side
se-resize bottom_right_corner
sw-resize bottom_left_corner
text xterm
w-resize left_side
wait watch
ALIASES
)
if [ ! -e "$icons/default/index.theme" ]; then
	printf '[Icon Theme]\nInherits=Adwaita\n' >"$icons/default/index.theme"
fi
