import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { TriangleAlert } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { type } from "@tauri-apps/plugin-os";
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
      className="flex items-center gap-2 px-4 py-2.5 text-[12px] leading-[1.4]"
    >
      <TriangleAlert
        size={14}
        className="shrink-0 text-warning"
        aria-hidden="true"
      />
      <p className="m-0 min-w-0 flex-1 text-ink-2">
        <span className="text-text">
          {t("settings.general.globeKey.title")}
        </span>{" "}
        {t("settings.general.globeKey.description")}
      </p>
      <button
        type="button"
        onClick={() => openUrl(KEYBOARD_SETTINGS_URL)}
        className="shrink-0 cursor-pointer rounded-[5px] px-1.5 py-0.5 text-[12px] font-medium text-logo-primary hover:bg-wash focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary"
      >
        {t("settings.general.globeKey.open")}
      </button>
    </div>
  );
};
