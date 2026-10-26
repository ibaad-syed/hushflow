import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands, type CleanupStatus } from "@/bindings";
import { formatModelSize } from "@/lib/utils/format";
import { SettingsGroup } from "../ui/SettingsGroup";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown } from "../ui/Dropdown";

interface Progress {
  id: string;
  downloaded: number;
  total: number;
}

/** "Clean up my text": on/off, which engine, and its download. */
export const CleanupSettings: React.FC = () => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<CleanupStatus | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);

  const refresh = useCallback(() => {
    commands.getCleanupStatus().then(setStatus);
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = [
      listen("cleanup-status-changed", refresh),
      listen<Progress>("cleanup-download-progress", (e) =>
        setProgress(e.payload),
      ),
      listen("cleanup-download-complete", () => {
        setProgress(null);
        refresh();
      }),
      listen("cleanup-download-failed", () => {
        setProgress(null);
        refresh();
      }),
    ];
    return () => {
      unlisten.forEach((p) => p.then((off) => off()));
    };
  }, [refresh]);

  if (!status) return null;
  const selected = status.engines.find((e) => e.id === status.engine);

  const state = (() => {
    if (!status.enabled || !selected) return "";
    if (progress?.id === selected.id || selected.downloading != null) {
      const done =
        progress?.id === selected.id
          ? progress.downloaded
          : (selected.downloading ?? 0);
      const total =
        progress?.id === selected.id
          ? progress.total
          : selected.size_mb * 1_000_000;
      return t("settings.cleanup.downloading", {
        percent: Math.min(99, Math.round((100 * done) / Math.max(total, 1))),
      });
    }
    if (!selected.downloaded) return t("settings.cleanup.notDownloaded");
    return status.ready
      ? t("settings.cleanup.ready")
      : t("settings.cleanup.loading");
  })();

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
      <SettingContainer
        title={t("settings.cleanup.engine.title")}
        description={t("settings.cleanup.engine.description")}
        descriptionMode="tooltip"
        grouped
        disabled={!status.enabled}
      >
        <div className="flex items-center gap-[10px]">
          {state && (
            <span className="text-small tabular-nums text-ink-3">{state}</span>
          )}
          <Dropdown
            selectedValue={status.engine}
            disabled={!status.enabled}
            options={status.engines.map((e) => ({
              value: e.id,
              label: e.recommended
                ? t("settings.cleanup.engine.recommendedLabel", {
                    name: e.name,
                  })
                : e.name,
              description: e.size_mb
                ? `${e.description} · ${formatModelSize(e.size_mb)}`
                : e.description,
            }))}
            onSelect={async (id) => {
              await commands.setCleanupEngine(id);
              refresh();
            }}
          />
        </div>
      </SettingContainer>
    </SettingsGroup>
  );
};
