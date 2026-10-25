import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { useSettings } from "../../../hooks/useSettings";

/** Let the app pick the engine: built-in first, then the recommended model. */
export const AutomaticEngine: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const automatic = getSetting("transcription_engine_mode") === "automatic";

  return (
    <ToggleSwitch
      checked={automatic}
      onChange={(on) =>
        updateSetting("transcription_engine_mode", on ? "automatic" : "manual")
      }
      isUpdating={isUpdating("transcription_engine_mode")}
      label={t("settings.models.automatic.title")}
      description={t("settings.models.automatic.description")}
      descriptionMode="inline"
    />
  );
};
