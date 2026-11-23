/* SPDX-License-Identifier: MIT */
#include "config.h"

#include <stdio.h>

#include "shell.h"

void
angi_panel_configure(struct desktop_shell *shell, const char *geometry)
{
	unsigned top, bottom, gap;
	char trailing;

	if (sscanf(geometry, "%3u %3u %3u%c", &top, &bottom, &gap, &trailing) != 3 ||
	    top > 256 || bottom > 256 || gap > 128)
		return;

	shell->angi_panel_top = top;
	shell->angi_panel_bottom = bottom;
	shell->angi_panel_gap = gap;
}

void
angi_panel_work_area(struct shell_surface *surface, pixman_rectangle32_t *area)
{
	struct desktop_shell *shell;
	unsigned gap, top, bottom;

	if (!surface || angi_is_desktop(surface))
		return;

	shell = surface->shell;
	gap = shell->angi_panel_gap;
	top = shell->angi_panel_top + gap;
	bottom = shell->angi_panel_bottom + gap;

	/* Keep small, rotated or temporarily disabled outputs usable. */
	if (area->width <= 2 * gap + 160 || area->height <= top + bottom + 120)
		return;

	area->x += gap;
	area->y += top;
	area->width -= 2 * gap;
	area->height -= top + bottom;
}
