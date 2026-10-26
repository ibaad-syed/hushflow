import React from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { useCleanupStatus } from "@/hooks/useCleanupStatus";
import { SettingsGroup } from "../ui/SettingsGroup";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { SettingContainer } from "../ui/SettingContainer";

/** General: the "Clean up my text" switch and which model is doing it.
 *  Models are picked and downloaded on the Models page. */
export const CleanupSettings: React.FC = () => {
  const { t } = useTranslation();
  const { status, refresh, stateOf } = useCleanupStatus();
  if (!status) return null;

  const engine = status.engines.find((e) => e.id === status.engine);
  const state = engine && stateOf(engine);
  const stateLabel =
    state?.kind === "downloading"
      ? t("settings.cleanup.downloading", { percent: state.percent })
      : state?.kind === "ready"
        ? t("settings.cleanup.ready")
        : state?.kind === "remote"
          ? t("settings.cleanup.notDownloaded")
          : t("settings.cleanup.loading");

  return (
    <SettingsGroup title={t("settings.cleanup.title")}>
      <ToggleSwitch
        checked={status.enabled}
        onChange={async (on) => {
          await commands.setCleanupEnabled(on);
          refresh();
        }}
        label={t("settings.cleanup.toggle.title")}
        description={t("settings.cleanup.toggle.description")}
        descriptionMode="tooltip"
        grouped
      />
      {status.enabled && engine && (
        <SettingContainer
          title={t("settings.cleanup.engine.title")}
          description={t("settings.cleanup.engine.description")}
          descriptionMode="tooltip"
          grouped
        >
          <span className="text-[13px] text-ink-2">
            {engine.name}
            <span className="text-ink-3">{` · ${stateLabel}`}</span>
          </span>
        </SettingContainer>
      )}
    </SettingsGroup>
  );
};
