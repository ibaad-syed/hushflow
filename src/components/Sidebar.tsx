import React from "react";
import { useTranslation } from "react-i18next";
import {
  BookOpen,
  Cog,
  Cpu,
  FlaskConical,
  History,
  Info,
  NotebookPen,
  SlidersHorizontal,
  Sparkles,
} from "lucide-react";
import { HushflowLogo } from "./icons/HushflowLogo";
import { useSettings } from "../hooks/useSettings";
import {
  GeneralSettings,
  AdvancedSettings,
  HistorySettings,
  DictionarySettings,
  MeetingsSettings,
  AboutSettings,
  PostProcessingSettings,
  ModelsSettings,
} from "./settings";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: any;
}

interface SectionConfig {
  /** Background of the section's icon tile, as in macOS System Settings. */
  tint: string;
  labelKey: string;
  icon: React.ComponentType<IconProps>;
  /** `null` when App renders the section itself (Debug, with its handlers). */
  component: React.ComponentType | null;
  enabled: (settings: any) => boolean;
}

export const SECTIONS_CONFIG = {
  general: {
    tint: "#8e8e93",
    labelKey: "sidebar.general",
    icon: SlidersHorizontal,
    component: GeneralSettings,
    enabled: () => true,
  },
  history: {
    tint: "#ff9f0a",
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    enabled: () => true,
  },
  meetings: {
    tint: "#ff453a",
    labelKey: "sidebar.meetings",
    icon: NotebookPen,
    component: MeetingsSettings,
    enabled: () => true,
  },
  dictionary: {
    tint: "#30b0c7",
    labelKey: "sidebar.dictionary",
    icon: BookOpen,
    component: DictionarySettings,
    enabled: () => true,
  },
  models: {
    tint: "#5e5ce6",
    labelKey: "sidebar.models",
    icon: Cpu,
    component: ModelsSettings,
    enabled: () => true,
  },
  advanced: {
    tint: "#636366",
    labelKey: "sidebar.advanced",
    icon: Cog,
    component: AdvancedSettings,
    enabled: () => true,
  },
  postprocessing: {
    tint: "#bf5af2",
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    component: PostProcessingSettings,
    enabled: (settings) => settings?.post_process_enabled ?? false,
  },
  debug: {
    tint: "#ff375f",
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: null,
    enabled: (settings) => settings?.debug_mode ?? false,
  },
  about: {
    tint: "#0a84ff",
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    enabled: () => true,
  },
} as const satisfies Record<string, SectionConfig>;

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.enabled(settings))
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));

  return (
    <nav className="flex h-full w-[220px] shrink-0 flex-col border-e border-line bg-sidebar px-2.5">
      <HushflowLogo className="px-2 pt-4 pb-4" />
      <div className="flex w-full flex-col gap-0.5">
        {availableSections.map((section) => {
          const Icon = section.icon;
          const isActive = activeSection === section.id;

          return (
            <button
              type="button"
              key={section.id}
              aria-current={isActive ? "page" : undefined}
              className={`flex h-[30px] w-full cursor-pointer items-center gap-2 rounded-[6px] px-1.5 text-start focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary ${
                isActive
                  ? "bg-background-ui text-white"
                  : "text-text hover:bg-wash"
              }`}
              onClick={() => onSectionChange(section.id)}
            >
              <span
                className="grid size-5 shrink-0 place-items-center rounded-[5px] text-white"
                style={{ background: section.tint }}
              >
                <Icon width={13} height={13} strokeWidth={2.2} />
              </span>
              <span
                className="truncate text-[13px]"
                title={t(section.labelKey)}
              >
                {t(section.labelKey)}
              </span>
            </button>
          );
        })}
      </div>
    </nav>
  );
};
