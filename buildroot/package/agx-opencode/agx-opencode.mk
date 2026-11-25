################################################################################
# Official standalone CLI. Keep its embedded runtime intact; do not strip.
################################################################################
AGX_OPENCODE_VERSION = 1.19.6
ifeq ($(BR2_x86_64),y)
AGX_OPENCODE_SOURCE = opencode-linux-x64-baseline.tar.gz
else
AGX_OPENCODE_SOURCE = opencode-linux-arm64.tar.gz
endif
AGX_OPENCODE_SITE = https://github.com/anomalyco/opencode/releases/download/v$(AGX_OPENCODE_VERSION)
AGX_OPENCODE_STRIP_COMPONENTS = 0
AGX_OPENCODE_LICENSE = MIT
AGX_OPENCODE_LICENSE_FILES = LICENSE

define AGX_OPENCODE_ADD_LICENSE
	cp $(BR2_EXTERNAL_AGX_PATH)/../apps/opencode/LICENSE.upstream $(@D)/LICENSE
endef
AGX_OPENCODE_POST_EXTRACT_HOOKS += AGX_OPENCODE_ADD_LICENSE
AGX_OPENCODE_DEPENDENCIES = bash git ripgrep ca-certificates

define AGX_OPENCODE_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/opencode $(TARGET_DIR)/usr/libexec/opencode/opencode
	$(INSTALL) -D -m 0755 $(BR2_EXTERNAL_AGX_PATH)/../apps/opencode/launch.sh $(TARGET_DIR)/usr/bin/opencode
	$(INSTALL) -D -m 0644 $(BR2_EXTERNAL_AGX_PATH)/../apps/opencode/LICENSE.upstream $(TARGET_DIR)/usr/share/licenses/opencode/LICENSE
	$(INSTALL) -d -m 1777 $(TARGET_DIR)/var/lib/agx-data/agx-opencode-users
endef

$(eval $(generic-package))
