import React from "react";
import { useTranslation } from "react-i18next";
import { Check, Download, Trash2 } from "lucide-react";
import { commands } from "@/bindings";
import { formatModelSize } from "@/lib/utils/format";
import { useCleanupStatus } from "@/hooks/useCleanupStatus";
import { SettingsGroup } from "../../ui/SettingsGroup";

/** Models page: the cleanup models, beside the speech models above. */
export const CleanupModels: React.FC = () => {
  const { t } = useTranslation();
  const { status, refresh, stateOf } = useCleanupStatus();
  if (!status) return null;

  const choose = async (id: string) => {
    await commands.setCleanupEngine(id);
    refresh();
  };

  return (
    <SettingsGroup
      title={t("settings.cleanup.models.title")}
      description={t("settings.cleanup.models.description")}
    >
      {status.engines.map((engine) => {
        const state = stateOf(engine);
        const inUse = status.enabled && status.engine === engine.id;
        return (
          <div
            key={engine.id}
            className="grid grid-cols-[20px_1fr_64px_auto] items-center gap-3 px-4 py-2.5"
          >
            <span
              className={`grid size-[18px] place-items-center rounded-full border ${
                inUse
                  ? "border-transparent bg-background-ui text-white"
                  : "border-line-strong"
              }`}
              aria-hidden="true"
            >
              {inUse && <Check size={11} strokeWidth={3} />}
            </span>
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <span className="truncate text-[13px] font-medium text-text">
                  {engine.name}
                </span>
                {engine.recommended && (
                  <span className="rounded-[4px] bg-logo-primary/15 px-1.5 py-px text-[11px] font-medium text-logo-primary">
                    {t("onboarding.cleanup.recommended")}
                  </span>
                )}
              </div>
              <p className="m-0 text-[12px] leading-[1.35] text-ink-3">
                {engine.description}
              </p>
            </div>
            {/* Its own column, so the size is never cut off. */}
            <span className="text-end text-[12px] tabular-nums text-ink-2">
              {engine.size_mb
                ? formatModelSize(engine.size_mb)
                : t("onboarding.cleanup.noDownload")}
            </span>
            <div className="flex items-center gap-2 text-[12px] tabular-nums text-ink-3">
              {state.kind === "downloading" && (
                <span>
                  {t("settings.cleanup.downloading", {
                    percent: state.percent,
                  })}
                </span>
              )}
              {state.kind === "loading" && (
                <span>{t("settings.cleanup.loading")}</span>
              )}
              {state.kind === "ready" && (
                <span className="text-moss">{t("settings.cleanup.ready")}</span>
              )}
              {!inUse && state.kind !== "downloading" && (
                <button
                  type="button"
                  onClick={() => choose(engine.id)}
                  className="inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-md border border-line-strong px-2.5 text-[12px] font-medium text-text hover:bg-wash focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary"
                >
                  {state.kind === "remote" && (
                    <Download size={13} aria-hidden="true" />
                  )}
                  {state.kind === "remote"
                    ? t("settings.cleanup.models.download")
                    : t("settings.cleanup.models.use")}
                </button>
              )}
              {!inUse && engine.size_mb > 0 && state.kind === "downloaded" && (
                <button
                  type="button"
                  aria-label={t("settings.cleanup.models.delete", {
                    name: engine.name,
                  })}
                  onClick={async () => {
                    await commands.deleteCleanupModel(engine.id);
                    refresh();
                  }}
                  className="grid size-7 cursor-pointer place-items-center rounded-md text-ink-3 hover:bg-wash hover:text-text focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary"
                >
                  <Trash2 size={14} aria-hidden="true" />
                </button>
              )}
            </div>
          </div>
        );
      })}
    </SettingsGroup>
  );
};
