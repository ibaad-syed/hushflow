import React from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { CustomWords } from "../CustomWords";

/** Names, products and jargon the user wants spelled their way. Speech-to-text
 * corrects near-misses to these, and cleanup is told to keep them as written. */
export const DictionarySettings: React.FC = () => {
  const { t } = useTranslation();
  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.dictionary.title")}>
        <CustomWords descriptionMode="inline" grouped />
      </SettingsGroup>
    </div>
  );
};
