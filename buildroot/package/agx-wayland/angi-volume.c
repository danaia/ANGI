/* SPDX-License-Identifier: MIT */
#include "config.h"

#include <linux/input-event-codes.h>
#include <string.h>
#include <libweston/desktop.h>
#include <libweston/shell-utils.h>

#include "angi-tiles.h"
#include "shared/helpers.h"

static const struct {
	uint32_t key;
	const char *message;
} volume_keys[] = {
	{ KEY_VOLUMEUP, "M up" },
	{ KEY_VOLUMEDOWN, "M down" },
	{ KEY_MUTE, "M mute" },
};

static void
position_hud(struct shell_surface *hud, struct weston_output *output)
{
	struct weston_coord_global position;

	if (!output)
		output = weston_shell_utils_get_default_output(hud->shell->compositor);
	if (!output)
		return;

	position = output->pos;
	position.c.x += (output->width - hud->view->surface->width) / 2;
	weston_view_set_position(hud->view, position);
}

static void
clear_hud_input(struct wl_listener *listener, void *data)
{
	struct weston_surface *surface = data;

	/* This runs after Weston applies the client's pending input region. */
	pixman_region32_clear(&surface->input);
}

bool
angi_volume_commit(struct shell_surface *surface)
{
	struct desktop_shell *shell = surface->shell;
	const char *title;

	if (shell->angi_volume == surface && weston_surface_is_mapped(surface->view->surface))
		return true;
	if (!shell->angi_volume_bindings[0] || !angi_is_desktop(surface) ||
	    (shell->angi_volume && shell->angi_volume != surface))
		return false;
	title = weston_desktop_surface_get_title(surface->desktop_surface);
	if (!title || strcmp(title, "ANGI Volume") ||
	    !weston_surface_has_content(surface->view->surface))
		return false;

	if (!shell->angi_volume) {
		shell->angi_volume = surface;
		shell->angi_volume_input.notify = clear_hud_input;
		wl_signal_add(&surface->view->surface->commit_signal, &shell->angi_volume_input);
	}
	position_hud(surface, NULL);
	weston_surface_map(surface->view->surface);
	weston_view_move_to_layer(surface->view, &shell->angi_controls_layer.view_list);
	return true;
}

void
angi_volume_remove(struct shell_surface *surface)
{
	struct desktop_shell *shell = surface->shell;

	if (shell->angi_volume != surface)
		return;
	wl_list_remove(&shell->angi_volume_input.link);
	shell->angi_volume = NULL;
}

static void
volume_pressed(struct weston_keyboard *keyboard, const struct timespec *time,
               uint32_t key, void *data)
{
	struct desktop_shell *shell = data;
	unsigned i;

	if (shell->locked)
		return;

	for (i = 0; i < ARRAY_LENGTH(volume_keys); i++) {
		if (volume_keys[i].key != key)
			continue;
		if (shell->angi_volume)
			position_hud(shell->angi_volume, keyboard->focus ? keyboard->focus->output : NULL);
		angi_tiles_send(shell, volume_keys[i].message, strlen(volume_keys[i].message));
		return;
	}
}

void
angi_volume_fini(struct desktop_shell *shell)
{
	unsigned i;

	for (i = 0; i < ARRAY_LENGTH(shell->angi_volume_bindings); i++) {
		if (shell->angi_volume_bindings[i])
			weston_binding_destroy(shell->angi_volume_bindings[i]);
		shell->angi_volume_bindings[i] = NULL;
	}
}

void
angi_volume_init(struct desktop_shell *shell)
{
	unsigned i;

	for (i = 0; i < ARRAY_LENGTH(volume_keys); i++) {
		shell->angi_volume_bindings[i] =
			weston_compositor_add_key_binding(shell->compositor, volume_keys[i].key,
			                                 0, volume_pressed, shell);
		if (!shell->angi_volume_bindings[i]) {
			angi_volume_fini(shell);
			return;
		}
	}
}
