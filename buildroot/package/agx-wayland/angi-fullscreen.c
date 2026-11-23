/* SPDX-License-Identifier: MIT */
#include "config.h"

#include <string.h>
#include <libweston/desktop.h>

#include "shell.h"

bool
angi_fullscreen_register(struct shell_surface *surface)
{
	struct desktop_shell *shell = surface->shell;
	const char *id = weston_desktop_surface_get_app_id(surface->desktop_surface);
	const char *title = weston_desktop_surface_get_title(surface->desktop_surface);

	if (shell->angi_controls == surface)
		return true;
	if (!shell->angi_workspace_source || !id || !title ||
	    strcmp(id, "org.angi.Desktop") || strcmp(title, "ANGI Fullscreen Controls"))
		return false;
	if (shell->angi_controls)
		return false;

	shell->angi_controls = surface;
	return true;
}

void
angi_fullscreen_hide(struct desktop_shell *shell)
{
	struct shell_surface *target = shell->angi_fullscreen_target;
	struct weston_seat *seat;

	if (!target)
		return;

	shell->angi_fullscreen_target = NULL;
	if (shell->angi_controls) {
		weston_view_move_to_layer(shell->angi_controls->view,
		                          &shell->angi_hidden.view_list);
		wl_list_for_each(seat, &shell->compositor->seat_list, link) {
			struct weston_keyboard *keyboard = weston_seat_get_keyboard(seat);
			if (keyboard && keyboard->focus == shell->angi_controls->view->surface)
				weston_keyboard_set_focus(keyboard, target->view->surface);
		}
	}
	weston_compositor_schedule_repaint(shell->compositor);
}

static struct shell_surface *
covering_window(struct desktop_shell *shell, struct weston_pointer *pointer,
                struct weston_output *output)
{
	struct shell_surface *target;
	struct weston_geometry geometry;
	struct weston_coord_global origin;
	const char *id;

	if (!pointer->focus)
		return NULL;
	if (pointer->focus == shell->angi_controls->view)
		target = shell->angi_fullscreen_target;
	else
		target = get_shell_surface(weston_surface_get_main_surface(pointer->focus->surface));
	if (!target || !target->desktop_surface ||
	    target->view->layer_link.layer != &shell->workspace.layer ||
	    weston_desktop_surface_get_maximized(target->desktop_surface))
		return NULL;

	id = weston_desktop_surface_get_app_id(target->desktop_surface);
	if (id && strcmp(id, "org.angi.Desktop") == 0)
		return NULL;

	/* Some video outputs cover the monitor with an ordinary toplevel. */
	geometry = weston_desktop_surface_get_geometry(target->desktop_surface);
	origin = weston_view_get_pos_offset_global(target->view);
	double left = origin.c.x + geometry.x;
	double top = origin.c.y + geometry.y;
	if (left > output->pos.c.x || top > output->pos.c.y ||
	    left + geometry.width < output->pos.c.x + output->width ||
	    top + geometry.height < output->pos.c.y + output->height)
		return NULL;

	return target;
}

static struct shell_surface *
fullscreen_at(struct desktop_shell *shell, struct weston_pointer *pointer,
              struct weston_output *output)
{
	struct weston_view *view;
	struct shell_surface *surface;

	wl_list_for_each(view, &shell->fullscreen_layer.view_list.link, layer_link.link) {
		surface = get_shell_surface(view->surface);
		if (surface && surface->view == view && surface->desktop_surface &&
		    surface->fullscreen_output &&
		    weston_desktop_surface_get_fullscreen(surface->desktop_surface) &&
		    surface->fullscreen_output->output == output)
			return surface;
	}
	return covering_window(shell, pointer, output);
}

void
angi_fullscreen_restore_covering_window(struct shell_surface *target)
{
	struct weston_output *output = target->view->output;
	struct weston_coord_global position;
	struct weston_geometry geometry;
	int width, height;

	if (!output)
		return;

	width = output->width * 3 / 4;
	height = output->height * 3 / 4;
	geometry = weston_desktop_surface_get_geometry(target->desktop_surface);
	position = output->pos;
	position.c.x += (output->width - width) / 2 - geometry.x;
	position.c.y += (output->height - height) / 2 - geometry.y;
	weston_view_set_position(target->view, position);
	weston_desktop_surface_set_size(target->desktop_surface, width, height);
}

static void
fullscreen_pointer_moved(struct wl_listener *listener, void *data)
{
	struct shell_seat *seat = wl_container_of(listener, seat, angi_motion);
	struct weston_pointer *pointer = data;
	struct desktop_shell *shell = seat->angi_shell;
	struct shell_surface *target;
	struct weston_output *output;
	struct weston_surface *strip;
	struct weston_coord_global position;
	double y;

	if (!shell->angi_controls || pointer->button_count || pointer->grab != &pointer->default_grab)
		return;

	wl_list_for_each(output, &shell->compositor->output_list, link) {
		if (weston_output_contains_coord(output, pointer->pos))
			break;
	}
	target = shell->locked || &output->link == &shell->compositor->output_list ?
		NULL : fullscreen_at(shell, pointer, output);
	if (!target) {
		angi_fullscreen_hide(shell);
		return;
	}
	strip = shell->angi_controls->view->surface;
	y = pointer->pos.c.y - output->pos.c.y;
	if (shell->angi_fullscreen_target == target && y < strip->height)
		return;
	if (y >= 3) {
		angi_fullscreen_hide(shell);
		return;
	}
	position = output->pos;
	position.c.x += (output->width - strip->width) / 2;
	shell->angi_fullscreen_target = target;
	weston_view_set_position(shell->angi_controls->view, position);
	weston_view_move_to_layer(shell->angi_controls->view,
	                          &shell->angi_controls_layer.view_list);
	weston_compositor_schedule_repaint(shell->compositor);
}

void
angi_fullscreen_detach_pointer(struct shell_seat *seat)
{
	wl_list_remove(&seat->angi_motion.link);
	wl_list_init(&seat->angi_motion.link);
	wl_list_remove(&seat->angi_pointer_destroy.link);
	wl_list_init(&seat->angi_pointer_destroy.link);
}

static void
fullscreen_pointer_destroyed(struct wl_listener *listener, void *data)
{
	struct shell_seat *seat = wl_container_of(listener, seat, angi_pointer_destroy);
	angi_fullscreen_detach_pointer(seat);
}

void
angi_fullscreen_watch_pointer(struct shell_seat *seat, struct weston_pointer *pointer)
{
	if (!pointer) {
		angi_fullscreen_detach_pointer(seat);
		angi_fullscreen_hide(seat->angi_shell);
	} else if (wl_list_empty(&seat->angi_motion.link)) {
		seat->angi_motion.notify = fullscreen_pointer_moved;
		wl_signal_add(&pointer->motion_signal, &seat->angi_motion);
		seat->angi_pointer_destroy.notify = fullscreen_pointer_destroyed;
		wl_signal_add(&pointer->destroy_signal, &seat->angi_pointer_destroy);
	}
}
