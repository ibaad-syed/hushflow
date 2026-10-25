import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { TriangleAlert } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { type } from "@tauri-apps/plugin-os";
import { Button } from "../ui/Button";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";

const KEYBOARD_SETTINGS_URL =
  "x-apple.systempreferences:com.apple.Keyboard-Settings.extension";

/** On macOS, an Fn shortcut also fires the Globe key's own action (emoji
 *  picker, input source, Apple dictation) unless that is set to Do Nothing. */
export const GlobeKeyNotice: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const binding = getSetting("bindings")?.transcribe?.current_binding ?? "";
  const usesFn = binding.split("+").includes("fn");
  const [conflict, setConflict] = useState(false);

  useEffect(() => {
    if (type() !== "macos" || !usesFn) {
      setConflict(false);
      return;
    }
    const check = () =>
      commands.globeKeyHasSystemAction().then(setConflict, () => {});
    check();
    // The fix happens in System Settings; notice it on the way back.
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, [usesFn]);

  if (!conflict) return null;
  return (
    <div
      role="status"
      className="mx-3 my-2 flex items-center gap-3 rounded-lg border border-transparent bg-warning-wash px-3 py-2.5"
    >
      <TriangleAlert className="h-5 w-5 shrink-0 text-warning" />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium leading-5">
          {t("settings.general.globeKey.title")}
        </p>
        <p className="text-sm leading-5 text-ink-2">
          {t("settings.general.globeKey.description")}
        </p>
      </div>
      <Button
        variant="warning"
        size="md"
        onClick={() => openUrl(KEYBOARD_SETTINGS_URL)}
        className="shrink-0"
      >
        {t("settings.general.globeKey.open")}
      </Button>
    </div>
  );
};
