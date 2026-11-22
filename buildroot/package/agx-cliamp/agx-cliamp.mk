################################################################################
# Official standalone ARM64 CLIamp release.
################################################################################
AGX_CLIAMP_VERSION = 2.4.1
AGX_CLIAMP_SOURCE = cliamp-linux-arm64
AGX_CLIAMP_SITE = https://github.com/bjarneo/cliamp/releases/download/v$(AGX_CLIAMP_VERSION)
AGX_CLIAMP_LICENSE = MIT
AGX_CLIAMP_LICENSE_FILES = LICENSE
AGX_CLIAMP_DEPENDENCIES = alsa-lib ca-certificates

define AGX_CLIAMP_EXTRACT_CMDS
	cp $(AGX_CLIAMP_DL_DIR)/$(AGX_CLIAMP_SOURCE) $(@D)/cliamp
	cp $(BR2_EXTERNAL_AGX_PATH)/../apps/sound/CLIamp.LICENSE $(@D)/LICENSE
endef

define AGX_CLIAMP_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/cliamp $(TARGET_DIR)/usr/libexec/agx-sound/cliamp
	$(INSTALL) -D -m 0755 $(BR2_EXTERNAL_AGX_PATH)/../apps/sound/cliamp.sh $(TARGET_DIR)/usr/bin/cliamp
	$(INSTALL) -D -m 0644 $(BR2_EXTERNAL_AGX_PATH)/../apps/sound/stations.m3u $(TARGET_DIR)/usr/share/agx/sound/stations.m3u
	$(INSTALL) -D -m 0644 $(BR2_EXTERNAL_AGX_PATH)/../apps/sound/radio-paradise.m3u $(TARGET_DIR)/usr/share/agx/sound/radio-paradise.m3u
	$(INSTALL) -D -m 0644 $(@D)/LICENSE $(TARGET_DIR)/usr/share/licenses/cliamp/LICENSE
endef

$(eval $(generic-package))
