/* SPDX-License-Identifier: MIT */
#include "config.h"

#include <math.h>
#include <stdio.h>

#include <libweston/desktop.h>
#include "shared/helpers.h"

#include "angi-tiles.h"

#define MAX_TILE_COORD 32768

static void
apply_region(struct angi_tile *tile, struct weston_geometry geometry)
{
	struct weston_view *view = tile->surface->view;
	pixman_region32_t *scissor = &view->geometry.scissor;
	pixman_box32_t *rects;
	int n, i;
	float sx = (float)geometry.width / tile->width;
	float sy = (float)geometry.height / tile->height;

	weston_view_set_mask(view, geometry.x, geometry.y, geometry.width, geometry.height);
	pixman_region32_clear(scissor);
	rects = pixman_region32_rectangles(&tile->clip, &n);
	for (i = 0; i < n; ++i) {
		int left = geometry.x + lroundf(rects[i].x1 * sx);
		int right = geometry.x + lroundf(rects[i].x2 * sx);
		int top = geometry.y + lroundf(rects[i].y1 * sy);
		int bottom = geometry.y + lroundf(rects[i].y2 * sy);
		if (left < right && top < bottom)
			pixman_region32_union_rect(scissor, scissor, left, top, right - left, bottom - top);
	}
}

static void
apply_geometry(struct angi_tile *tile)
{
	struct shell_surface *surface = tile->surface;
	struct weston_view *view = surface->view;
	struct weston_view *desktop = angi_tile_desktop_view(surface->shell);
	struct weston_geometry geometry = weston_desktop_surface_get_geometry(surface->desktop_surface);
	struct weston_coord_global position;
	float sx, sy;

	if (!desktop || !tile->positioned || !tile->width || !tile->height ||
	    geometry.width <= 0 || geometry.height <= 0)
		return;

	/* Keep the slot filled between configure and commit, or if a client
	 * enforces a minimum size. Weston inverse-transforms pointer input. */
	sx = (float)tile->width / geometry.width;
	sy = (float)tile->height / geometry.height;
	wl_list_remove(&tile->scale.link);
	wl_list_init(&tile->scale.link);
	if (sx != 1.0f || sy != 1.0f) {
		weston_matrix_init(&tile->scale.matrix);
		weston_matrix_scale(&tile->scale.matrix, sx, sy, 1.0f);
		wl_list_insert(&view->geometry.transformation_list, &tile->scale.link);
	}
	position = weston_view_get_pos_offset_global(desktop);
	position.c.x += tile->x - geometry.x * sx;
	position.c.y += tile->y - geometry.y * sy;
	weston_view_set_position(view, position);
	apply_region(tile, geometry);
	weston_view_geometry_dirty(view);
	tile->applied_geometry = geometry;
}

bool
angi_tile_committed(struct shell_surface *surface)
{
	struct angi_tile *tile = surface->angi_tile;
	struct weston_geometry geometry;

	if (!tile)
		return false;
	geometry = weston_desktop_surface_get_geometry(surface->desktop_surface);
	if (geometry.x != tile->applied_geometry.x || geometry.y != tile->applied_geometry.y ||
	    geometry.width != tile->applied_geometry.width || geometry.height != tile->applied_geometry.height)
		apply_geometry(tile);
	return true;
}

static bool
valid_box(int x, int y, int width, int height)
{
	return x >= -MAX_TILE_COORD && x <= MAX_TILE_COORD &&
	       y >= -MAX_TILE_COORD && y <= MAX_TILE_COORD &&
	       width >= 0 && width <= MAX_TILE_COORD &&
	       height >= 0 && height <= MAX_TILE_COORD;
}

static void
round_bottom(pixman_region32_t *clip, int width, int height, int radius)
{
	int y, inset;

	radius = MIN(radius, MIN(width / 2, height));
	for (y = 0; y < radius; ++y) {
		double dy = y + 0.5;
		inset = radius - (int)sqrt(radius * radius - dy * dy);
		if (inset < 1)
			continue;
		pixman_region32_subtract(clip, clip, &(pixman_region32_t) {
			.extents = { 0, height - radius + y, inset, height - radius + y + 1 }
		});
		pixman_region32_subtract(clip, clip, &(pixman_region32_t) {
			.extents = { width - inset, height - radius + y, width, height - radius + y + 1 }
		});
	}
}

void
angi_tile_position(struct desktop_shell *shell, const char *message)
{
	struct shell_surface *surface, *child;
	struct angi_tile *tile;
	unsigned id, workspace, full;
	int x, y, width, height, radius, offset = 0;

	if (sscanf(message, "%u %u %d %d %d %d %u %d%n",
	           &id, &workspace, &x, &y, &width, &height, &full, &radius, &offset) != 8 ||
	    !valid_box(x, y, width, height) || workspace > 9 || full > 1 || radius < 0 || radius > 256)
		return;
	surface = angi_tile_find(shell, id);
	if (!surface)
		return;
	tile = surface->angi_tile;
	surface->angi_workspace = workspace;
	tile->positioned = width > 0 && height > 0;
	if ((!tile->positioned || !full) && shell->angi_fullscreen_target == surface)
		angi_fullscreen_hide(shell);
	tile->x = x;
	tile->y = y;
	tile->width = width;
	tile->height = height;
	if (tile->positioned) {
		if (tile->fullscreen != full) {
			tile->fullscreen = full;
			weston_desktop_surface_set_fullscreen(surface->desktop_surface, full);
		}
		pixman_region32_fini(&tile->clip);
		pixman_region32_init_rect(&tile->clip, 0, 0, width, height);
		if (!full)
			round_bottom(&tile->clip, width, height, radius);
		message += offset;
		while (*message) {
			int left, top, w, h;
			pixman_region32_t cover;
			if (sscanf(message, "%d %d %d %d%n", &left, &top, &w, &h, &offset) != 4 ||
			    !valid_box(left, top, w, h))
				break;
			pixman_region32_init_rect(&cover, left - x, top - y, w, h);
			pixman_region32_subtract(&tile->clip, &tile->clip, &cover);
			pixman_region32_fini(&cover);
			message += offset;
		}
		if (tile->applied_width != width || tile->applied_height != height) {
			weston_desktop_surface_set_size(surface->desktop_surface, width, height);
			tile->applied_width = width;
			tile->applied_height = height;
		}
		apply_geometry(tile);
	}
	shell_surface_update_layer(surface);
	wl_list_for_each(child, &shell->shsurf_list, link) {
		if (child != surface && child->desktop_surface && angi_tile_owner(child) == surface) {
			child->angi_workspace = workspace;
			if (weston_view_is_mapped(child->view))
				shell_surface_update_layer(child);
		}
	}
	weston_compositor_schedule_repaint(shell->compositor);
}

