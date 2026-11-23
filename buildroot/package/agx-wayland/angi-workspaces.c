/* SPDX-License-Identifier: MIT */
#include "config.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>

#include <libweston/desktop.h>

#include "shell.h"

static bool
angi_is_desktop(struct shell_surface *surface)
{
	const char *id = weston_desktop_surface_get_app_id(surface->desktop_surface);

	return id && strcmp(id, "org.angi.Desktop") == 0;
}

bool
angi_workspace_visible(struct shell_surface *surface)
{
	return surface->angi_workspace == surface->shell->angi_workspace ||
	       angi_is_desktop(surface);
}

static void
apply_workspace(struct desktop_shell *shell, unsigned workspace,
                int removed)
{
	struct shell_surface *shsurf, *desktop = NULL;
	struct weston_seat *seat;
	unsigned target;

	if (removed < 0 && shell->angi_workspace == workspace)
		return;

	shell->angi_workspace = workspace;
	angi_fullscreen_hide(shell);
	wl_list_for_each_reverse(shsurf, &shell->shsurf_list, link) {
		bool hidden;

		if (removed >= 0) {
			target = shsurf->angi_workspace;
			if (target > (unsigned)removed ||
			    (target == (unsigned)removed && removed > 0))
				target--;
			shsurf->angi_workspace = target;
		}

		if (!shsurf->desktop_surface || !weston_view_is_mapped(shsurf->view) ||
		    shell->angi_controls == shsurf)
			continue;

		if (shsurf->view->layer_link.layer == &shell->minimized_layer)
			continue;

		hidden = shsurf->view->layer_link.layer == &shell->angi_hidden;
		if (hidden == angi_workspace_visible(shsurf))
			shell_surface_update_layer(shsurf);
		if (angi_is_desktop(shsurf))
			desktop = shsurf;
	}

	/* Never leave keyboard focus on a window hidden by the switch. */
	wl_list_for_each(seat, &shell->compositor->seat_list, link) {
		struct weston_keyboard *keyboard = weston_seat_get_keyboard(seat);
		if (!keyboard || !keyboard->focus)
			continue;

		shsurf = get_shell_surface(weston_surface_get_main_surface(keyboard->focus));
		if (shsurf && !angi_workspace_visible(shsurf)) {
			if (desktop)
				activate(shell, desktop->view, seat, WESTON_ACTIVATE_FLAG_NONE);
			else
				weston_keyboard_set_focus(keyboard, NULL);
		}
	}

	weston_compositor_schedule_repaint(shell->compositor);
}

static bool
workspace_digit(char c)
{
	return c >= '0' && c <= '9';
}

static int
workspace_message(int fd, uint32_t mask, void *data)
{
	struct desktop_shell *shell = data;
	char message[4] = { 0 };
	ssize_t len = recv(fd, message, sizeof(message), MSG_DONTWAIT | MSG_TRUNC);

	/* S<n> selects a workspace. R<n> <n> removes one and selects the
	 * new current workspace in the same operation. IDs are zero-based. */
	if (len == 2 && message[0] == 'S' && workspace_digit(message[1]))
		apply_workspace(shell, message[1] - '0', -1);
	if (len == 4 && message[0] == 'R' && workspace_digit(message[1]) &&
	    message[2] == ' ' && workspace_digit(message[3]))
		apply_workspace(shell, message[3] - '0', message[1] - '0');
	if (len == 2 && message[0] == 'F' && message[1] >= '0' && message[1] <= '2')
		angi_fullscreen_action(shell, message[1] - '0');

	return 0;
}

void
angi_workspaces_init(struct desktop_shell *shell)
{
	const char *display = getenv("WAYLAND_DISPLAY");
	const char *runtime = getenv("XDG_RUNTIME_DIR");
	struct sockaddr_un addr = { .sun_family = AF_UNIX };
	struct wl_event_loop *loop;
	int fd, len;

	if (!display || !*display || (*display != '/' && (!runtime || !*runtime)))
		return;

	/* The launcher gives each compositor a private, mode-0700 runtime directory. */
	len = (*display == '/') ?
		snprintf(addr.sun_path, sizeof(addr.sun_path), "%s.angi2", display) :
		snprintf(addr.sun_path, sizeof(addr.sun_path), "%s/%s.angi2", runtime, display);
	if (len < 0 || (size_t)len >= sizeof(addr.sun_path))
		return;

	fd = socket(AF_UNIX, SOCK_DGRAM | SOCK_CLOEXEC | SOCK_NONBLOCK, 0);
	if (fd < 0)
		return;
	if (bind(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0)
		goto close_socket;

	shell->angi_workspace_path = strdup(addr.sun_path);
	if (!shell->angi_workspace_path || chmod(addr.sun_path, 0600) < 0)
		goto unlink_socket;

	loop = wl_display_get_event_loop(shell->compositor->wl_display);
	shell->angi_workspace_source =
		wl_event_loop_add_fd(loop, fd, WL_EVENT_READABLE, workspace_message, shell);
	if (shell->angi_workspace_source) {
		shell->angi_workspace_fd = fd;
		return;
	}

unlink_socket:
	unlink(addr.sun_path);
	free(shell->angi_workspace_path);
	shell->angi_workspace_path = NULL;
close_socket:
	close(fd);
}

void
angi_workspaces_fini(struct desktop_shell *shell)
{
	if (!shell->angi_workspace_source)
		return;

	wl_event_source_remove(shell->angi_workspace_source);
	close(shell->angi_workspace_fd);
	unlink(shell->angi_workspace_path);
	free(shell->angi_workspace_path);
}
