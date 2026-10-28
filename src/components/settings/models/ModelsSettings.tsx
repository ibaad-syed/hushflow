import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";
import { Tooltip } from "@/components/ui/Tooltip";
import { useModelStore } from "@/stores/modelStore";
import {
  ModelTable,
  Note,
  useModelFilters,
  type TableSection,
} from "@/components/shared/modelTable/ModelTable";
import RetiredModelsNotice from "./RetiredModelsNotice";
import { ModelRow } from "./ModelRow";
import { Legend } from "./Legend";
import { useModelEntries } from "./modelEntries";
import { AutomaticEngine } from "./AutomaticEngine";
import { CleanupModels } from "./CleanupModels";
import { CleanupInstructions } from "./CleanupInstructions";

/** Past this many models on disk, the rest fold under "Show N more". */
const YOURS_LIMIT = 5;

/** Look for model files added outside Hushflow: quiet, beside its section's name. */
const Rescan: React.FC = () => {
  const { t } = useTranslation();
  const rescanning = useModelStore((s) => s.isRescanning);
  return (
    <Tooltip text={t("settings.models.rescan.tooltip")}>
      <button
        type="button"
        onClick={() => useModelStore.getState().rescanLocalModels()}
        disabled={rescanning}
        aria-label={t("settings.models.rescan.tooltip")}
        className="ms-[6px] grid size-[22px] cursor-pointer place-items-center rounded-full text-ink-3 hover:bg-wash hover:text-text disabled:cursor-default focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary"
      >
        <RefreshCw
          className={`size-[12px] ${rescanning ? "animate-spin" : ""}`}
          aria-hidden="true"
        />
      </button>
    </Tooltip>
  );
};

interface ModelsSettingsProps {
  /** Where the languages are changed (General). */
  onChangeLanguages?: () => void;
}

/** The Models page: onboarding's model table, filling the window. */
export const ModelsSettings: React.FC<ModelsSettingsProps> = ({
  onChangeLanguages,
}) => {
  const { t } = useTranslation();
  const loading = useModelStore((s) => s.loading);
  const { facts, lists } = useModelEntries();
  const { filters, setFilters, searching, matches } = useModelFilters();
  const [more, setMore] = useState(false);
  const [tab, setTab] = useState<"speech" | "cleanup">("speech");

  if (loading) {
    return (
      <div className="max-w-3xl w-full mx-auto">
        <div className="flex items-center justify-center py-16">
          <div className="w-8 h-8 border-2 border-logo-primary border-t-transparent rounded-full animate-spin" />
        </div>
      </div>
    );
  }

  const keep = (xs: typeof lists.yours) => xs.filter(matches);
  const moreButton = !searching && lists.yours.length > YOURS_LIMIT && (
    <button
      type="button"
      onClick={() => setMore((m) => !m)}
      className="ms-[40px] mt-[2px] flex h-[28px] cursor-pointer items-center gap-[8px] rounded-[7px] px-[2px] text-small font-medium text-ink-2 hover:text-text focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary"
    >
      {more
        ? t("settings.models.showFewer")
        : t("settings.models.showMore", {
            count: lists.yours.length - YOURS_LIMIT,
          })}
      <span
        aria-hidden="true"
        className={`inline-block size-[6px] shrink-0 border-e-[1.5px] border-b-[1.5px] border-current ${more ? "mt-[3px] -rotate-[135deg]" : "-mt-[3px] rotate-45"}`}
      />
    </button>
  );

  const sections: TableSection[] = searching
    ? [
        {
          key: "yours",
          label: t("settings.models.sections.yours"),
          rows: keep(lists.yours),
        },
        {
          key: "fits",
          label: t("settings.models.sections.fits"),
          rows: keep(lists.fits),
        },
        {
          key: "partial",
          label: t("modelTable.missingLanguage"),
          rows: keep(lists.partial),
        },
        {
          key: "untested",
          label: t("settings.models.sections.untested"),
          rows: filters.query.trim() ? keep(lists.untested) : [],
          note: t("settings.models.untestedNote"),
        },
      ]
    : [
        {
          key: "yours",
          label: (
            <>
              {t("settings.models.sections.yours")}
              <Rescan />
            </>
          ),
          rows: more ? lists.yours : lists.yours.slice(0, YOURS_LIMIT),
          after: lists.yours.length ? (
            moreButton
          ) : (
            <Note>{t("settings.models.emptyYours")}</Note>
          ),
        },
        {
          key: "fits",
          label: t("settings.models.sections.fits"),
          rows: lists.fits,
        },
        {
          key: "partial",
          label: t("modelTable.missingLanguage"),
          rows: lists.partial,
        },
      ];

  return (
    <div className="mx-auto flex min-h-0 w-full max-w-3xl flex-1 flex-col">
      {/* The title, and across from it where to change the languages the
          grades are for. */}
      <div className="mb-[16px] flex flex-none items-center justify-between gap-[12px] px-[12px]">
        <h1 className="m-0 text-[20px] font-bold tracking-[-0.01em]">
          {t("settings.models.title")}
        </h1>
        {onChangeLanguages && tab === "speech" && (
          <button
            type="button"
            onClick={onChangeLanguages}
            className="shrink-0 cursor-pointer text-sm text-text underline decoration-line-strong underline-offset-[3px] hover:decoration-text focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary"
          >
            {t("settings.models.changeLanguages")}
          </button>
        )}
      </div>
      {/* macOS segmented control: speech-to-text models | cleanup models. */}
      <div
        role="tablist"
        aria-label={t("settings.models.title")}
        className="mx-[12px] mb-[16px] grid flex-none grid-cols-2 self-center rounded-[7px] bg-wash p-[2px] text-[12px] font-medium"
      >
        {(["speech", "cleanup"] as const).map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            onClick={() => setTab(id)}
            className={`h-[22px] min-w-[120px] cursor-pointer rounded-[5px] px-3 focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary ${
              tab === id
                ? "bg-panel text-text shadow-[0_1px_2px_rgba(0,0,0,0.25)]"
                : "text-ink-2 hover:text-text"
            }`}
          >
            {t(`settings.models.tabs.${id}`)}
          </button>
        ))}
      </div>
      {tab === "cleanup" ? (
        <div className="mx-[12px] flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto pb-4">
          <CleanupModels />
          <CleanupInstructions />
        </div>
      ) : (
        <>
          <div className="mx-[12px] mb-[16px] flex-none">
            <AutomaticEngine />
          </div>
          <div className="mx-[12px] mb-[16px] flex-none empty:hidden">
            <RetiredModelsNotice />
          </div>
          <ModelTable
            grading={facts}
            filters={filters}
            setFilters={setFilters}
            // "Yours" always shows at rest; a search hides empty sections.
            sections={sections.filter(
              (s) => s.rows.length || (s.key === "yours" && !searching),
            )}
            renderRow={(e) => <ModelRow key={e.id} entry={e} />}
            actionsHead={<Legend tier={facts.tier} />}
            footer={
              !searching &&
              lists.untested.length > 0 && (
                <Note>
                  {t("settings.models.untestedHidden", {
                    count: lists.untested.length,
                  })}
                </Note>
              )
            }
            thinEdge
          />
        </>
      )}
    </div>
  );
};
