################################################################################
# AGX local Rust workspace; scripts/prepare-rust-package.sh vendors locked crates.
################################################################################
AGX_VERSION = 0.4.0
AGX_SITE = $(BR2_EXTERNAL_AGX_PATH)/../.cache/agx-package
AGX_SITE_METHOD = local
AGX_DEPENDENCIES += libxcrypt dbus host-pkgconf
# agx-bluetoothd links libdbus-1 through BlueR; the target pkg-config wrapper finds it in staging.
AGX_CARGO_ENV = PKG_CONFIG_ALLOW_CROSS=1
AGX_CARGO_BUILD_OPTS = --workspace

define AGX_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/target/$(RUSTC_TARGET_NAME)/$(if $(BR2_ENABLE_DEBUG),debug,release)/agx-display-devices $(TARGET_DIR)/usr/libexec/agx-display-devices
	$(INSTALL) -D -m 0755 $(@D)/target/$(RUSTC_TARGET_NAME)/$(if $(BR2_ENABLE_DEBUG),debug,release)/agx-init $(TARGET_DIR)/usr/sbin/agx-init
	$(INSTALL) -D -m 0755 $(@D)/target/$(RUSTC_TARGET_NAME)/$(if $(BR2_ENABLE_DEBUG),debug,release)/agx $(TARGET_DIR)/usr/bin/agx
	$(INSTALL) -D -m 0755 $(@D)/target/$(RUSTC_TARGET_NAME)/$(if $(BR2_ENABLE_DEBUG),debug,release)/agx-netd $(TARGET_DIR)/usr/sbin/agx-netd
	$(INSTALL) -D -m 0755 $(@D)/target/$(RUSTC_TARGET_NAME)/$(if $(BR2_ENABLE_DEBUG),debug,release)/agx-dhcp $(TARGET_DIR)/usr/sbin/agx-dhcp
	$(INSTALL) -D -m 0755 $(@D)/target/$(RUSTC_TARGET_NAME)/$(if $(BR2_ENABLE_DEBUG),debug,release)/agx-bluetoothd $(TARGET_DIR)/usr/sbin/agx-bluetoothd
	$(INSTALL) -D -m 0755 $(@D)/target/$(RUSTC_TARGET_NAME)/$(if $(BR2_ENABLE_DEBUG),debug,release)/agx-securityd $(TARGET_DIR)/usr/sbin/agx-securityd
endef

$(eval $(cargo-package))
