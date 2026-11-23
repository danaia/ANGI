/* Apply ANGI's saved mouse preferences in the compositor that owns libinput. */

#include "config.h"

#include <fcntl.h>
#include <libinput.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "libinput-device.h"
#include "libinput-seat.h"

struct angi_mouse {
	char *path;
	struct wl_event_source *timer;
	struct stat observed;
	bool valid;
	bool adaptive;
	bool left;
	bool natural;
	int speed;
};

static bool
parse_switch(const char *value, bool *result)
{
	if (strcmp(value, "true") == 0)
		*result = true;
	else if (strcmp(value, "false") == 0)
		*result = false;
	else
		return false;
	return true;
}

static bool
read_preferences(struct angi_mouse *mouse)
{
	struct stat status;
	char buffer[512], *line, *next, *value, *end;
	bool adaptive = true, left = false, natural = false;
	long speed = 0;
	ssize_t length;
	int fd;

	if (stat(mouse->path, &status) < 0 || !S_ISREG(status.st_mode) ||
	    (mouse->observed.st_dev == status.st_dev &&
	     mouse->observed.st_ino == status.st_ino &&
	     mouse->observed.st_mtim.tv_sec == status.st_mtim.tv_sec &&
	     mouse->observed.st_mtim.tv_nsec == status.st_mtim.tv_nsec))
		return false;

	fd = open(mouse->path, O_RDONLY | O_CLOEXEC | O_NONBLOCK);
	if (fd < 0)
		return false;
	if (fstat(fd, &status) < 0 || !S_ISREG(status.st_mode)) {
		close(fd);
		return false;
	}
	length = read(fd, buffer, sizeof buffer);
	close(fd);
	mouse->observed = status;
	if (length <= 0 || length >= (ssize_t) sizeof buffer)
		return false;
	buffer[length] = '\0';

	for (line = strtok_r(buffer, "\n", &next); line;
	     line = strtok_r(NULL, "\n", &next)) {
		value = strchr(line, '=');
		if (!value)
			return false;
		*value++ = '\0';
		if (strcmp(line, "speed") == 0) {
			speed = strtol(value, &end, 10);
			if (end == value || *end || speed < -100 || speed > 100)
				return false;
		} else if (strcmp(line, "adaptive") == 0) {
			if (!parse_switch(value, &adaptive))
				return false;
		} else if (strcmp(line, "left") == 0) {
			if (!parse_switch(value, &left))
				return false;
		} else if (strcmp(line, "natural") == 0) {
			if (!parse_switch(value, &natural))
				return false;
		}
	}

	mouse->valid = true;
	mouse->speed = speed;
	mouse->adaptive = adaptive;
	mouse->left = left;
	mouse->natural = natural;
	return true;
}

void
angi_mouse_apply(struct udev_input *input, struct libinput_device *device)
{
	struct angi_mouse *mouse = input->angi_mouse;
	enum libinput_config_accel_profile profile;

	if (!mouse || !mouse->valid)
		return;
	if (libinput_device_config_accel_is_available(device)) {
		profile = mouse->adaptive ? LIBINPUT_CONFIG_ACCEL_PROFILE_ADAPTIVE :
					   LIBINPUT_CONFIG_ACCEL_PROFILE_FLAT;
		if (libinput_device_config_accel_get_profiles(device) & profile)
			libinput_device_config_accel_set_profile(device, profile);
		libinput_device_config_accel_set_speed(device, mouse->speed / 100.0);
		weston_log("ANGI mouse: %s acceleration %s, speed %.2f\n",
			   libinput_device_get_name(device),
			   libinput_device_config_accel_get_profile(device) ==
			   LIBINPUT_CONFIG_ACCEL_PROFILE_ADAPTIVE ? "adaptive" : "flat",
			   libinput_device_config_accel_get_speed(device));
	}
	if (libinput_device_config_left_handed_is_available(device))
		libinput_device_config_left_handed_set(device, mouse->left);
	if (libinput_device_config_scroll_has_natural_scroll(device))
		libinput_device_config_scroll_set_natural_scroll_enabled(device, mouse->natural);
}

static int
refresh_preferences(void *data)
{
	struct udev_input *input = data;
	struct angi_mouse *mouse = input->angi_mouse;
	struct udev_seat *seat;
	struct evdev_device *device;

	if (read_preferences(mouse)) {
		wl_list_for_each(seat, &input->compositor->seat_list, base.link) {
			wl_list_for_each(device, &seat->devices_list, link)
				angi_mouse_apply(input, device->device);
		}
	}
	wl_event_source_timer_update(mouse->timer, 250);
	return 0;
}

void
angi_mouse_init(struct udev_input *input)
{
	const char *path = getenv("AGX_WAYLAND_MOUSE_CONFIG");
	struct angi_mouse *mouse;

	if (!path || !*path)
		return;
	mouse = calloc(1, sizeof *mouse);
	if (!mouse)
		return;
	mouse->path = strdup(path);
	if (!mouse->path) {
		free(mouse);
		return;
	}
	mouse->timer = wl_event_loop_add_timer(
		wl_display_get_event_loop(input->compositor->wl_display),
		refresh_preferences, input);
	if (!mouse->timer) {
		free(mouse->path);
		free(mouse);
		return;
	}
	input->angi_mouse = mouse;
	read_preferences(mouse);
	wl_event_source_timer_update(mouse->timer, 250);
}

void
angi_mouse_fini(struct udev_input *input)
{
	if (!input->angi_mouse)
		return;
	wl_event_source_remove(input->angi_mouse->timer);
	free(input->angi_mouse->path);
	free(input->angi_mouse);
	input->angi_mouse = NULL;
}
