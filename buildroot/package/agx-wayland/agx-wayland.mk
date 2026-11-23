################################################################################
# Weston for ANGI's libudev-only device stack.
################################################################################
AGX_WAYLAND_VERSION = 15.0.1
AGX_WAYLAND_SITE = https://gitlab.freedesktop.org/wayland/weston/-/releases/$(AGX_WAYLAND_VERSION)/downloads
AGX_WAYLAND_SOURCE = weston-$(AGX_WAYLAND_VERSION).tar.xz
AGX_WAYLAND_CURSOR_VERSION = 48.1
AGX_WAYLAND_CURSOR_SOURCE = adwaita-icon-theme-$(AGX_WAYLAND_CURSOR_VERSION)
AGX_WAYLAND_EXTRA_DOWNLOADS = https://download.gnome.org/sources/adwaita-icon-theme/48/$(AGX_WAYLAND_CURSOR_SOURCE).tar.xz
AGX_WAYLAND_LICENSE = MIT, LGPL-3.0 or CC-BY-SA-3.0 (cursor assets)
AGX_WAYLAND_LICENSE_FILES = COPYING $(addprefix $(AGX_WAYLAND_CURSOR_SOURCE)/,COPYING COPYING_LGPL COPYING_CCBYSA3)
AGX_WAYLAND_DEPENDENCIES = host-pkgconf wayland wayland-protocols libudev \
	libxkbcommon pixman libpng cairo libinput libdisplay-info libdrm \
	libegl libgbm libgles seatd
AGX_WAYLAND_CONF_OPTS = \
	-Dbackend-default=drm -Dbackend-drm=true -Dbackend-headless=true \
	-Dbackend-wayland=false -Dbackend-x11=false -Dbackend-rdp=false \
	-Dbackend-pipewire=false -Dbackend-vnc=false -Dxwayland=false \
	-Drenderer-gl=true -Drenderer-vulkan=false -Dpipewire=false \
	-Dshell-desktop=true -Dshell-ivi=false -Dshell-kiosk=false -Dshell-lua=false \
	-Ddeprecated-shell-fullscreen=false -Ddeprecated-screenshare=false \
	-Dcolor-management-lcms=false -Dimage-jpeg=false -Dimage-webp=false \
	-Ddoc=false -Dremoting=false -Dsystemd=false -Dtest-junit-xml=false \
	-Dsimple-clients=shm -Ddemo-clients=false -Dtools=debug,terminal

define AGX_WAYLAND_EXTRACT_CURSORS
	tar -xJf $(AGX_WAYLAND_DL_DIR)/$(AGX_WAYLAND_CURSOR_SOURCE).tar.xz -C $(@D) \
		--exclude='*.cur' $(addprefix $(AGX_WAYLAND_CURSOR_SOURCE)/,Adwaita/cursors COPYING COPYING_LGPL COPYING_CCBYSA3)
endef
AGX_WAYLAND_POST_EXTRACT_HOOKS += AGX_WAYLAND_EXTRACT_CURSORS

define AGX_WAYLAND_COPY_WORKSPACE_BRIDGE
	cp $(AGX_WAYLAND_PKGDIR)/angi-workspaces.c $(AGX_WAYLAND_PKGDIR)/angi-fullscreen.c $(@D)/desktop-shell/
endef
AGX_WAYLAND_POST_PATCH_HOOKS += AGX_WAYLAND_COPY_WORKSPACE_BRIDGE

define AGX_WAYLAND_INSTALL_SESSION
	$(INSTALL) -D -m 0755 $(AGX_WAYLAND_PKGDIR)/angi-wayland $(TARGET_DIR)/usr/bin/angi-wayland
	$(INSTALL) -D -m 0755 $(AGX_WAYLAND_PKGDIR)/agx-wayland-shell $(TARGET_DIR)/usr/libexec/agx-wayland-shell
	$(INSTALL) -D -m 0644 $(AGX_WAYLAND_PKGDIR)/weston.ini $(TARGET_DIR)/usr/share/agx/wayland/weston.ini
	sh $(AGX_WAYLAND_PKGDIR)/install-cursors.sh $(@D)/$(AGX_WAYLAND_CURSOR_SOURCE) $(TARGET_DIR)
endef
AGX_WAYLAND_POST_INSTALL_TARGET_HOOKS += AGX_WAYLAND_INSTALL_SESSION

# Weston owns the compositor. Qt clients only need Qt's Wayland client plugins.
ifeq ($(BR2_PACKAGE_AGX_WAYLAND)$(BR2_PACKAGE_QT5WAYLAND_COMPOSITOR),y)
QT5WAYLAND_CONF_OPTS += -- -no-feature-wayland-server
endif

$(eval $(meson-package))
