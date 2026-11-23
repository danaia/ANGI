/* SPDX-License-Identifier: MIT */
#include "config.h"

#include <linux/input-event-codes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>

#include <libweston/desktop.h>
#include "shared/helpers.h"

#include "angi-tiles.h"

#define TILE_LIMIT 56
#define TILE_TEXT_LIMIT 256

static const struct {
	uint32_t code;
	const char *name;
} shortcut[] = {
	{ KEY_SPACE, "space" }, { KEY_Q, "q" }, { KEY_W, "w" }, { KEY_J, "j" },
	{ KEY_T, "t" }, { KEY_F, "f" }, { KEY_L, "l" },
	{ KEY_A, "a" }, { KEY_B, "b" }, { KEY_C, "c" }, { KEY_D, "d" }, { KEY_E, "e" },
	{ KEY_G, "g" }, { KEY_H, "h" }, { KEY_I, "i" }, { KEY_K, "k" }, { KEY_M, "m" },
	{ KEY_N, "n" }, { KEY_O, "o" }, { KEY_P, "p" }, { KEY_R, "r" }, { KEY_S, "s" },
	{ KEY_U, "u" }, { KEY_V, "v" }, { KEY_X, "x" }, { KEY_Y, "y" }, { KEY_Z, "z" },
	{ KEY_F1, "f1" }, { KEY_F2, "f2" }, { KEY_F3, "f3" }, { KEY_F4, "f4" },
	{ KEY_F5, "f5" }, { KEY_F6, "f6" }, { KEY_F7, "f7" }, { KEY_F8, "f8" },
	{ KEY_F9, "f9" }, { KEY_F10, "f10" }, { KEY_F11, "f11" }, { KEY_F12, "f12" },
	{ KEY_F13, "f13" }, { KEY_F14, "f14" }, { KEY_F15, "f15" }, { KEY_F16, "f16" },
	{ KEY_F17, "f17" }, { KEY_F18, "f18" }, { KEY_F19, "f19" }, { KEY_F20, "f20" },
	{ KEY_F21, "f21" }, { KEY_F22, "f22" }, { KEY_F23, "f23" }, { KEY_F24, "f24" },
	{ KEY_ESC, "escape" }, { KEY_BACKSPACE, "backspace" }, { KEY_DELETE, "delete" },
	{ KEY_HOME, "home" }, { KEY_END, "end" }, { KEY_PAGEUP, "pageup" }, { KEY_PAGEDOWN, "pagedown" },
	{ KEY_SLASH, "/" }, { KEY_DOT, "." }, { KEY_COMMA, "," }, { KEY_SEMICOLON, ";" },
	{ KEY_LEFTBRACE, "[" }, { KEY_RIGHTBRACE, "]" }, { KEY_APOSTROPHE, "'" },
	{ KEY_BACKSLASH, "\\" }, { KEY_GRAVE, "`" },
	{ KEY_LEFT, "left" }, { KEY_RIGHT, "right" },
	{ KEY_UP, "up" }, { KEY_DOWN, "down" },
	{ KEY_ENTER, "enter" }, { KEY_TAB, "tab" }, { KEY_MINUS, "-" }, { KEY_EQUAL, "=" },
	{ KEY_0, "0" }, { KEY_1, "1" }, { KEY_2, "2" }, { KEY_3, "3" }, { KEY_4, "4" },
	{ KEY_5, "5" }, { KEY_6, "6" }, { KEY_7, "7" }, { KEY_8, "8" }, { KEY_9, "9" },
};

struct angi_tiles {
	struct weston_layer layer;
	struct sockaddr_un peer;
	socklen_t peer_len;
	uint32_t next_id, focus, pending_focus;
	unsigned count;
	struct weston_binding *bindings[160];
	unsigned bindings_len;
	uint16_t shortcut_mask[ARRAY_LENGTH(shortcut)];
};

static bool
send_message(struct desktop_shell *shell, const char *message, size_t length)
{
	struct angi_tiles *tiles = shell->angi_tiles;

	return tiles && tiles->peer_len &&
	       sendto(shell->angi_workspace_fd, message, length, MSG_DONTWAIT,
	              (const struct sockaddr *)&tiles->peer, tiles->peer_len) == (ssize_t)length;
}

static char *
hex_field(char *dest, const char *text)
{
	static const char hex[] = "0123456789abcdef";
	unsigned n;

	if (!text || !*text) {
		*dest++ = '-';
		return dest;
	}
	for (n = 0; text[n] && n < TILE_TEXT_LIMIT; ++n) {
		unsigned char byte = text[n];
		*dest++ = hex[byte >> 4];
		*dest++ = hex[byte & 15];
	}
	return dest;
}

static void
publish_windows(struct desktop_shell *shell)
{
	struct angi_tiles *tiles = shell->angi_tiles;
	struct shell_surface *surface;
	char message[64 + TILE_LIMIT * (40 + 4 * TILE_TEXT_LIMIT)];
	char *cursor;

	if (!tiles || !tiles->peer_len)
		return;

	cursor = message + sprintf(message, "V %u\n", tiles->pending_focus ? tiles->pending_focus : tiles->focus);
	wl_list_for_each_reverse(surface, &shell->shsurf_list, link) {
		if (!surface->angi_tile)
			continue;
		cursor += sprintf(cursor, "%u %u ", surface->angi_tile->id, surface->angi_workspace);
		cursor = hex_field(cursor, weston_desktop_surface_get_app_id(surface->desktop_surface));
		*cursor++ = ' ';
		cursor = hex_field(cursor, weston_desktop_surface_get_title(surface->desktop_surface));
		*cursor++ = '\n';
	}
	send_message(shell, message, cursor - message);
}

static struct shell_surface *
parent_surface(struct shell_surface *surface)
{
	struct weston_desktop_surface *parent;

	if (!surface->desktop_surface)
		return NULL;
	parent = weston_desktop_surface_get_parent(surface->desktop_surface);
	return parent ? weston_desktop_surface_get_user_data(parent) : NULL;
}

struct shell_surface *
angi_tile_owner(struct shell_surface *surface)
{
	while (surface && !surface->angi_tile)
		surface = parent_surface(surface);
	return surface;
}

struct weston_layer_entry *
angi_tile_layer(struct shell_surface *surface)
{
	struct shell_surface *owner = angi_tile_owner(surface);

	if (!owner)
		return NULL;
	return &owner->shell->angi_tiles->layer.view_list;
}

bool
angi_tile_visible(struct shell_surface *surface)
{
	struct shell_surface *owner = angi_tile_owner(surface);

	return !owner || (owner->angi_tile->positioned && owner->angi_workspace == owner->shell->angi_workspace);
}

bool
angi_tile_fullscreen(struct shell_surface *surface)
{
	return surface && surface->angi_tile && surface->angi_tile->fullscreen &&
	       surface->angi_tile->positioned;
}

static void
metadata_changed(struct wl_listener *listener, void *data)
{
	struct angi_tile *tile = wl_container_of(listener, tile, metadata);

	publish_windows(tile->surface->shell);
}

static bool
register_tile(struct shell_surface *surface)
{
	struct desktop_shell *shell = surface->shell;
	struct angi_tiles *tiles = shell->angi_tiles;
	struct angi_tile *tile;

	if (surface->angi_tile)
		return true;
	if (!tiles || !tiles->peer_len || tiles->count >= TILE_LIMIT ||
	    tiles->next_id >= INT32_MAX || angi_is_desktop(surface) || parent_surface(surface) ||
	    surface->xwayland.is_set)
		return false;

	tile = calloc(1, sizeof *tile);
	if (!tile)
		return false;

	surface->angi_tile = tile;
	tile->surface = surface;
	tile->id = ++tiles->next_id;
	tiles->count++;
	pixman_region32_init(&tile->clip);
	wl_list_init(&tile->scale.link);
	tile->metadata.notify = metadata_changed;
	weston_desktop_surface_add_metadata_listener(surface->desktop_surface, &tile->metadata);
	/* Tiles replace the client's outer decoration and geometry state. */
	weston_desktop_surface_set_maximized(surface->desktop_surface, false);
	weston_desktop_surface_set_fullscreen(surface->desktop_surface, false);
	weston_desktop_surface_set_orientation(surface->desktop_surface,
		WESTON_TOP_LEVEL_TILED_ORIENTATION_LEFT | WESTON_TOP_LEVEL_TILED_ORIENTATION_RIGHT |
		WESTON_TOP_LEVEL_TILED_ORIENTATION_TOP | WESTON_TOP_LEVEL_TILED_ORIENTATION_BOTTOM);
	return true;
}

bool
angi_tile_map(struct shell_surface *surface)
{
	struct weston_surface *content = surface->view->surface;
	if (!register_tile(surface))
		return false;

	weston_surface_map(content);
	shell_surface_update_layer(surface);
	surface->shell->angi_tiles->pending_focus = surface->angi_tile->id;
	publish_windows(surface->shell);
	return true;
}

void
angi_tile_remove(struct shell_surface *surface)
{
	struct angi_tile *tile = surface->angi_tile;
	struct angi_tiles *tiles = surface->shell->angi_tiles;

	if (!tile)
		return;
	surface->angi_tile = NULL;
	tiles->count--;
	wl_list_remove(&tile->metadata.link);
	wl_list_remove(&tile->scale.link);
	pixman_region32_fini(&tile->clip);
	if (tiles->focus == tile->id) {
		tiles->focus = 0;
		while (tiles->bindings_len)
			weston_binding_destroy(tiles->bindings[--tiles->bindings_len]);
	}
	if (tiles->pending_focus == tile->id)
		tiles->pending_focus = 0;
	free(tile);
	publish_windows(surface->shell);
}

void
angi_tile_parent_changed(struct shell_surface *surface)
{
	if (!surface->angi_tile)
		return;
	angi_tile_remove(surface);
	weston_view_set_mask_infinite(surface->view);
	weston_desktop_surface_set_orientation(surface->desktop_surface,
		WESTON_TOP_LEVEL_TILED_ORIENTATION_NONE);
	weston_view_geometry_dirty(surface->view);
}

struct weston_view *
angi_tile_desktop_view(struct desktop_shell *shell)
{
	struct shell_surface *surface;

	wl_list_for_each(surface, &shell->shsurf_list, link) {
		if (surface->desktop_surface && angi_is_desktop(surface) &&
		    surface != shell->angi_controls && weston_view_is_mapped(surface->view))
			return surface->view;
	}
	return NULL;
}

struct shell_surface *
angi_tile_find(struct desktop_shell *shell, unsigned id)
{
	struct shell_surface *surface;
	wl_list_for_each(surface, &shell->shsurf_list, link) {
		if (surface->angi_tile && surface->angi_tile->id == id)
			return surface;
	}
	return NULL;
}

static void
shortcut_pressed(struct weston_keyboard *keyboard, const struct timespec *time,
                 uint32_t key, void *data)
{
	struct desktop_shell *shell = data;
	unsigned state = keyboard->seat->modifier_state;
	unsigned mods = (state & MODIFIER_CTRL ? 1 : 0) | (state & MODIFIER_ALT ? 2 : 0) |
	                (state & MODIFIER_SHIFT ? 4 : 0) | (state & MODIFIER_SUPER ? 8 : 0);
	unsigned i;
	char buffer[32];

	for (i = 0; i < ARRAY_LENGTH(shortcut); ++i) {
		if (shortcut[i].code == key) {
			int n = snprintf(buffer, sizeof buffer, "K %s %u", shortcut[i].name, mods);
			send_message(shell, buffer, n);
			return;
		}
	}
}

static void
set_shortcuts(struct desktop_shell *shell, bool active)
{
	struct angi_tiles *tiles = shell->angi_tiles;
	unsigned i, mods;

	if (!active) {
		while (tiles->bindings_len)
			weston_binding_destroy(tiles->bindings[--tiles->bindings_len]);
		return;
	}
	if (tiles->bindings_len)
		return;
	for (i = 0; i < ARRAY_LENGTH(shortcut); ++i) {
		for (mods = 1; mods < 16; ++mods) {
			struct weston_binding *binding;
			unsigned state;

			if (!(tiles->shortcut_mask[i] & (1u << mods)) ||
			    tiles->bindings_len == ARRAY_LENGTH(tiles->bindings))
				continue;
			state = (mods & 1 ? MODIFIER_CTRL : 0) | (mods & 2 ? MODIFIER_ALT : 0) |
			        (mods & 4 ? MODIFIER_SHIFT : 0) | (mods & 8 ? MODIFIER_SUPER : 0);
			binding = weston_compositor_add_key_binding(shell->compositor, shortcut[i].code,
				state, shortcut_pressed, shell);
			if (binding)
				tiles->bindings[tiles->bindings_len++] = binding;
		}
	}
}

static void
configure_shortcuts(struct desktop_shell *shell, const char *text)
{
	struct angi_tiles *tiles = shell->angi_tiles;
	uint16_t shortcuts[ARRAY_LENGTH(shortcut)] = { 0 };
	char key[24];
	unsigned bits, i;
	int consumed;

	while (sscanf(text, " %23s %u%n", key, &bits, &consumed) == 2) {
		if (bits < 16 && (bits & 11)) {
			for (i = 0; i < ARRAY_LENGTH(shortcut); ++i) {
				if (!strcmp(key, shortcut[i].name)) {
					shortcuts[i] |= 1u << bits;
					break;
				}
			}
		}
		text += consumed;
	}
	if (memcmp(shortcuts, tiles->shortcut_mask, sizeof shortcuts) == 0)
		return;
	set_shortcuts(shell, false);
	memcpy(tiles->shortcut_mask, shortcuts, sizeof shortcuts);
	if (tiles->focus)
		set_shortcuts(shell, true);
}

void
angi_tile_activated(struct shell_surface *surface)
{
	struct angi_tiles *tiles = surface->shell->angi_tiles;
	struct shell_surface *owner = angi_tile_owner(surface);
	uint32_t id = owner ? owner->angi_tile->id : 0;

	if (!tiles || (tiles->focus == id && !tiles->pending_focus))
		return;
	tiles->focus = id;
	tiles->pending_focus = 0;
	set_shortcuts(surface->shell, id != 0);
	publish_windows(surface->shell);
}

bool
angi_tile_request(struct shell_surface *surface, const char *request)
{
	char message[32];
	int count;

	if (!surface || !surface->angi_tile)
		return false;
	count = snprintf(message, sizeof message, "X %u %s", surface->angi_tile->id, request);
	send_message(surface->shell, message, count);
	return true;
}

void
angi_tile_fullscreen_action(struct shell_surface *surface, unsigned action)
{
	char message[8];
	int count = snprintf(message, sizeof message, "F %u", action);
	send_message(surface->shell, message, count);
}

void
angi_tiles_message(struct desktop_shell *shell, const char *message,
                   const struct sockaddr_un *peer, socklen_t peer_len)
{
	struct angi_tiles *tiles = shell->angi_tiles;
	struct shell_surface *surface;
	struct weston_view *view;
	struct weston_seat *seat;
	unsigned id;
	char trailing;

	if (!tiles)
		return;
	if ((message[0] == 'Q' && (!message[1] || message[1] == ' ')) && peer_len > offsetof(struct sockaddr_un, sun_path) &&
	    peer_len <= sizeof tiles->peer && peer->sun_family == AF_UNIX) {
		tiles->peer = *peer;
		tiles->peer_len = peer_len;
		configure_shortcuts(shell, message + 1);
		wl_list_for_each(surface, &shell->shsurf_list, link) {
			if (surface->desktop_surface && weston_view_is_mapped(surface->view) &&
			    !surface->angi_tile && surface != shell->angi_controls && register_tile(surface))
				shell_surface_update_layer(surface);
		}
		publish_windows(shell);
		return;
	}
	if (!tiles->peer_len || peer_len != tiles->peer_len ||
	    memcmp(peer, &tiles->peer, peer_len) != 0)
		return;
	if (message[0] == 'G') {
		angi_tile_position(shell, message + 1);
		return;
	}
	if ((message[0] != 'I' && message[0] != 'C') ||
	    sscanf(message + 1, "%u%c", &id, &trailing) != 1)
		return;
	surface = angi_tile_find(shell, id);
	if (message[0] == 'C') {
		if (surface)
			weston_desktop_surface_close(surface->desktop_surface);
		return;
	}
	if (shell->locked || (!id && !tiles->focus))
		return;
	view = id ? surface && angi_tile_visible(surface) ? surface->view : NULL : angi_tile_desktop_view(shell);
	if (!view)
		return;
	wl_list_for_each(seat, &shell->compositor->seat_list, link)
		activate(shell, view, seat, WESTON_ACTIVATE_FLAG_NONE);
}

void
angi_tiles_init(struct desktop_shell *shell)
{
	struct angi_tiles *tiles;

	if (!(shell->compositor->capabilities & WESTON_CAP_VIEW_CLIP_MASK))
		return;
	tiles = calloc(1, sizeof *tiles);
	if (!tiles)
		return;
	shell->angi_tiles = tiles;
	weston_layer_init(&tiles->layer, shell->compositor);
	weston_layer_set_position(&tiles->layer, WESTON_LAYER_POSITION_NORMAL + 1);
}

void
angi_tiles_fini(struct desktop_shell *shell)
{
	struct angi_tiles *tiles = shell->angi_tiles;
	if (!tiles)
		return;
	set_shortcuts(shell, false);
	weston_layer_fini(&tiles->layer);
	free(tiles);
	shell->angi_tiles = NULL;
}
