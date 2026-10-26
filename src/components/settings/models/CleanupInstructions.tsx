import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { useCleanupStatus } from "@/hooks/useCleanupStatus";
import { SettingsGroup } from "../../ui/SettingsGroup";

/** The instructions the cleanup model follows, which the user can rewrite. */
export const CleanupInstructions: React.FC = () => {
  const { t } = useTranslation();
  const { status, refresh } = useCleanupStatus();
  const [draft, setDraft] = useState("");

  useEffect(() => {
    if (status) setDraft(status.prompt);
  }, [status?.prompt]);

  if (!status) return null;
  const changed = draft.trim() !== status.prompt.trim();

  const save = async (prompt: string | null) => {
    await commands.setCleanupPrompt(prompt);
    refresh();
  };

  return (
    <SettingsGroup
      title={t("settings.cleanup.instructions.title")}
      description={t("settings.cleanup.instructions.description")}
    >
      <div className="flex flex-col gap-2 p-3">
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          spellCheck={false}
          rows={12}
          aria-label={t("settings.cleanup.instructions.title")}
          className="w-full resize-y rounded-md border border-line bg-background px-3 py-2 font-mono text-[12px] leading-[1.5] text-text focus:border-logo-primary focus:outline-none"
        />
        <div className="flex items-center gap-2">
          <p className="m-0 flex-1 text-[12px] leading-[1.35] text-ink-3">
            {status.prompt_is_custom
              ? t("settings.cleanup.instructions.customNote")
              : t("settings.cleanup.instructions.defaultNote")}
          </p>
          {status.prompt_is_custom && (
            <button
              type="button"
              onClick={() => save(null)}
              className="h-7 cursor-pointer rounded-md px-2.5 text-[12px] font-medium text-ink-2 hover:bg-wash hover:text-text focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary"
            >
              {t("settings.cleanup.instructions.reset")}
            </button>
          )}
          <button
            type="button"
            disabled={!changed}
            onClick={() => save(draft)}
            className="h-7 cursor-pointer rounded-md bg-background-ui px-3 text-[12px] font-medium text-white hover:bg-background-ui/85 disabled:cursor-default disabled:bg-wash disabled:text-ink-3 focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary"
          >
            {t("settings.cleanup.instructions.save")}
          </button>
        </div>
      </div>
    </SettingsGroup>
  );
};
