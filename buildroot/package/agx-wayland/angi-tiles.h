/* SPDX-License-Identifier: MIT */
#ifndef ANGI_TILES_H
#define ANGI_TILES_H

#include "shell.h"

struct angi_tile {
	struct shell_surface *surface;
	uint32_t id;
	struct wl_listener metadata;
	struct weston_transform scale;
	struct weston_geometry applied_geometry;
	struct wl_event_source *resize_timer;
	bool resize_pending;
	int applied_width, applied_height;
	int committed_width, committed_height;
	int configured_from_width, configured_from_height;
	int64_t configured_ms;
	bool awaiting_commit;
	int x, y, width, height;
	bool fullscreen, positioned;
	pixman_region32_t clip;
};

struct weston_view *angi_tile_desktop_view(struct desktop_shell *shell);
struct shell_surface *angi_tile_find(struct desktop_shell *shell, unsigned id);
struct shell_surface *angi_tile_owner(struct shell_surface *surface);
void angi_tile_position(struct desktop_shell *shell, const char *message);

#endif
