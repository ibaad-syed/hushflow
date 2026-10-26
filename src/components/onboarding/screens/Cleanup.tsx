import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Sparkles } from "lucide-react";
import { Button } from "../../ui/Button";
import { OnboardingShell, ScreenHeader, ScreenMain } from "../OnboardingShell";
import { commands, type CleanupEngineInfo } from "@/bindings";
import { formatModelSize } from "@/lib/utils/format";

const OFF = "off";

interface CleanupProps {
  progress: { index: number; count: number };
  preview: boolean;
  onBack: () => void;
  onContinue: () => void;
}

/** Shown in a preview, where the backend isn't asked. */
const PREVIEW_ENGINES: CleanupEngineInfo[] = [
  {
    id: "gemma-4-e2b",
    name: "Gemma 4 E2B",
    description: "Best balance of quality and speed",
    size_mb: 3110,
    recommended: true,
    downloaded: false,
    downloading: null,
  },
];

/** Screen: pick how dictation gets polished. The download runs in the
 *  background; until it's done, dictation pastes the rule-based text. */
export const Cleanup: React.FC<CleanupProps> = ({
  progress,
  preview,
  onBack,
  onContinue,
}) => {
  const { t } = useTranslation();
  const [engines, setEngines] = useState<CleanupEngineInfo[]>([]);
  const [choice, setChoice] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (preview) {
      setEngines(PREVIEW_ENGINES);
      setChoice(PREVIEW_ENGINES[0].id);
      return;
    }
    commands.getCleanupStatus().then((status) => {
      setEngines(status.engines);
      setChoice(
        status.enabled
          ? status.engine
          : (status.engines.find((e) => e.recommended)?.id ?? OFF),
      );
    });
  }, [preview]);

  const chosen = engines.find((e) => e.id === choice);
  const needsDownload = !!chosen && !chosen.downloaded;

  const save = async () => {
    if (preview || !choice) return onContinue();
    setSaving(true);
    try {
      if (choice === OFF) await commands.setCleanupEnabled(false);
      else await commands.setCleanupEngine(choice);
    } finally {
      setSaving(false);
    }
    onContinue();
  };

  const options = [
    ...engines.map((e) => ({
      id: e.id,
      name: e.name,
      description: e.description,
      meta: e.size_mb
        ? formatModelSize(e.size_mb)
        : t("onboarding.cleanup.noDownload"),
      recommended: e.recommended,
    })),
    {
      id: OFF,
      name: t("onboarding.cleanup.off.name"),
      description: t("onboarding.cleanup.off.description"),
      meta: "",
      recommended: false,
    },
  ];

  return (
    <OnboardingShell
      progress={progress}
      onBack={onBack}
      hint={needsDownload ? t("onboarding.cleanup.background") : ""}
      primary={
        <Button
          variant="primary"
          size="lg"
          disabled={!choice || saving}
          onClick={save}
        >
          {t("onboarding.shell.continue")}
        </Button>
      }
    >
      <ScreenMain
        header={
          <ScreenHeader
            title={t("onboarding.cleanup.title")}
            sub={t("onboarding.cleanup.subtitle")}
          />
        }
      >
        <div
          role="radiogroup"
          aria-label={t("onboarding.cleanup.title")}
          className="flex w-full flex-col rounded-panel border border-line-strong px-[10px] py-[6px] text-start"
        >
          {options.map((o) => {
            const selected = o.id === choice;
            return (
              <button
                key={o.id}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => setChoice(o.id)}
                className={`my-[2px] grid cursor-pointer grid-cols-[34px_1fr_auto] items-center gap-[14px] rounded-[10px] px-[12px] py-[11px] text-start transition-colors focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary ${
                  selected ? "bg-wash" : "hover:bg-wash"
                }`}
              >
                <span
                  className={`grid size-[34px] place-items-center rounded-[9px] ${
                    selected
                      ? "bg-background-ui text-white"
                      : "bg-wash text-ink-3"
                  }`}
                >
                  {selected ? (
                    <Check size={17} aria-hidden="true" />
                  ) : (
                    <Sparkles size={16} aria-hidden="true" />
                  )}
                </span>
                <span className="min-w-0">
                  <span className="flex items-center gap-[8px]">
                    <b className="font-display text-row font-[650] leading-[1.3]">
                      {o.name}
                    </b>
                    {o.recommended && (
                      <span className="rounded-full bg-logo-primary/25 px-[8px] py-[1px] text-label font-semibold text-text">
                        {t("onboarding.cleanup.recommended")}
                      </span>
                    )}
                  </span>
                  <span className="mt-[1px] block text-small leading-[1.45] text-ink-2">
                    {o.description}
                  </span>
                </span>
                <span className="text-small tabular-nums text-ink-3">
                  {o.meta}
                </span>
              </button>
            );
          })}
        </div>
      </ScreenMain>
    </OnboardingShell>
  );
};
