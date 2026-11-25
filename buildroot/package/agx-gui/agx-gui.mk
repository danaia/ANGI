################################################################################
# ANGI desktop, built from its standalone Cargo workspace.
################################################################################
AGX_GUI_VERSION = 0.4.0
AGX_GUI_SITE = $(BR2_EXTERNAL_AGX_PATH)/../.cache/agx-package
AGX_GUI_SITE_METHOD = local
AGX_GUI_SUBDIR = crates/agx-gui
AGX_GUI_DEPENDENCIES = libxcrypt fontconfig host-pkgconf
AGX_GUI_CARGO_ENV = PKG_CONFIG_ALLOW_CROSS=1
ifeq ($(BR2_PACKAGE_AGX_GUI_HEADLESS),y)
AGX_GUI_CARGO_BUILD_OPTS = --no-default-features --features headless
else
AGX_GUI_DEPENDENCIES += libinput libxkbcommon libudev libdrm
AGX_GUI_CARGO_BUILD_OPTS = --no-default-features --features kms
ifeq ($(BR2_PACKAGE_AGX_GUI_PROVISION_GRAPHICS),y)
AGX_GUI_CARGO_BUILD_OPTS += --features kms-provision-graphics
# gbm-sys passes -lgbm. The vendored backend resolves the functions via dlopen.
# An empty library satisfies the linker's filename lookup and stays in the build tree.
define AGX_GUI_LINK_RUNTIME_GRAPHICS
	mkdir -p $(@D)/graphics-link
	printf '' | $(TARGET_CC) -x c -shared -nostdlib -Wl,-soname,libgbm.so.1 -o $(@D)/graphics-link/libgbm.so -
endef
AGX_GUI_PRE_BUILD_HOOKS += AGX_GUI_LINK_RUNTIME_GRAPHICS
AGX_GUI_CARGO_ENV += CARGO_TARGET_$(call UPPERCASE,$(RUSTC_TARGET_NAME))_RUSTFLAGS="-Lnative=$(@D)/graphics-link"
define AGX_GUI_VERIFY_RUNTIME_GRAPHICS
	if $(TARGET_READELF) -d $(@D)/$(AGX_GUI_SUBDIR)/target/$(RUSTC_TARGET_NAME)/$(if $(BR2_ENABLE_DEBUG),debug,release)/agx-gui | grep -E 'NEEDED.*lib(gbm|EGL|GLES)'; then \
		echo "ANGI setup must start without the graphics driver libraries" >&2; exit 1; \
	fi
endef
AGX_GUI_POST_BUILD_HOOKS += AGX_GUI_VERIFY_RUNTIME_GRAPHICS
else
AGX_GUI_DEPENDENCIES += mesa3d
endif
ifeq ($(BR2_PACKAGE_AGX_WAYLAND),y)
AGX_GUI_DEPENDENCIES += wayland
AGX_GUI_CARGO_BUILD_OPTS += --features wayland
define AGX_GUI_INSTALL_WAYLAND_ENTRY
	mkdir -p $(TARGET_DIR)/usr/libexec
	ln -sf agx-wayland-shell $(TARGET_DIR)/usr/libexec/agx-gui-wayland
endef
AGX_GUI_POST_INSTALL_TARGET_HOOKS += AGX_GUI_INSTALL_WAYLAND_ENTRY
endif
endif

define AGX_GUI_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/$(AGX_GUI_SUBDIR)/target/$(RUSTC_TARGET_NAME)/$(if $(BR2_ENABLE_DEBUG),debug,release)/agx-gui $(TARGET_DIR)/usr/bin/agx-gui
	$(INSTALL) -D -m 0644 $(@D)/$(AGX_GUI_SUBDIR)/ui/fonts/JetBrainsMonoNerdFont-Regular.ttf $(TARGET_DIR)/usr/share/fonts/JetBrainsMonoNerdFont-Regular.ttf
	ln -sf agx-gui $(TARGET_DIR)/usr/bin/open
endef

$(eval $(cargo-package))
