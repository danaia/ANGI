AGX_AGENT_VERSION = 0.9.5
AGX_AGENT_SOURCE = herdr-linux-aarch64
AGX_AGENT_SITE = https://github.com/herdrdev/herdr/releases/download/v$(AGX_AGENT_VERSION)
AGX_AGENT_EXTRA_DOWNLOADS = https://registry.npmjs.org/monaco-editor/-/monaco-editor-0.52.2.tgz
AGX_AGENT_LICENSE = Apache-2.0, MIT
AGX_AGENT_LICENSE_FILES = LICENSE monaco/LICENSE monaco/ThirdPartyNotices.txt
AGX_AGENT_DEPENDENCIES = agx-opencode python3
AGX_AGENT_ASSETS = $(BR2_EXTERNAL_AGX_PATH)/../apps/agent

define AGX_AGENT_EXTRACT_CMDS
	cp $(AGX_AGENT_DL_DIR)/$(AGX_AGENT_SOURCE) $(@D)/herdr
	cp $(AGX_AGENT_ASSETS)/LICENSE.herdr $(@D)/LICENSE
endef

define AGX_AGENT_BUILD_CMDS
	bash $(AGX_AGENT_ASSETS)/../../scripts/stage-monaco.sh $(@D)/monaco $(AGX_AGENT_DL_DIR)/monaco-editor-0.52.2.tgz
	$(TARGET_CC) $(TARGET_CFLAGS) $(TARGET_LDFLAGS) -Os -Wall -Wextra -Werror $(AGX_AGENT_ASSETS)/angi-root.c -o $(@D)/angi-root
endef

define AGX_AGENT_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m0755 $(@D)/herdr $(TARGET_DIR)/usr/bin/herdr
	$(INSTALL) -D -m0755 $(@D)/angi-root $(TARGET_DIR)/usr/bin/angi-root
	$(INSTALL) -D -m0755 $(AGX_AGENT_ASSETS)/agent.sh $(TARGET_DIR)/usr/bin/agent
	$(INSTALL) -D -m0755 $(AGX_AGENT_ASSETS)/angi-build.sh $(TARGET_DIR)/usr/bin/angi-build
	$(INSTALL) -D -m0755 $(AGX_AGENT_ASSETS)/angi-update.sh $(TARGET_DIR)/usr/bin/angi-update
	$(INSTALL) -d $(TARGET_DIR)/usr/libexec/agx-agent $(TARGET_DIR)/usr/share/agx/agent
	$(INSTALL) -m0644 $(AGX_AGENT_ASSETS)/*.py $(TARGET_DIR)/usr/libexec/agx-agent/
	$(INSTALL) -d $(TARGET_DIR)/usr/share/agx/agent/work-editor
	cp -R $(@D)/monaco/. $(TARGET_DIR)/usr/share/agx/agent/work-editor/
	cp -RL $(AGX_AGENT_ASSETS)/skills $(AGX_AGENT_ASSETS)/profiles $(TARGET_DIR)/usr/share/agx/agent/
	$(INSTALL) -m0644 $(AGX_AGENT_ASSETS)/herdr.toml $(AGX_AGENT_ASSETS)/sdk-env.sh $(AGX_AGENT_ASSETS)/LICENSE.herdr $(AGX_AGENT_ASSETS)/apps.json $(TARGET_DIR)/usr/share/agx/agent/
endef

define AGX_AGENT_PERMISSIONS
	/usr/bin/angi-root f 4750 0 1000 - - - - -
endef

$(eval $(generic-package))
