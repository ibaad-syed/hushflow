import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ProgressBar } from "../shared";
import { useSettings } from "../../hooks/useSettings";
import { useUpdateStore } from "@/stores/updateStore";
import { PORTABLE_RELEASES_URL } from "./portableInstaller";

/** How often a running app looks for a new release, besides at launch. It
 *  lives in the tray for days, so launch alone would miss releases. */
const CHECK_INTERVAL_MS = 4 * 60 * 60 * 1000;

interface UpdateCheckerProps {
  className?: string;
}

/** Whether update checks are on, once settings (and the lock) are known. */
export const useUpdateChecksEnabled = () => {
  const { settings, isLoading, updateChecksLocked } = useSettings();
  // Wait for the lock state too (null = not loaded yet), otherwise the first
  // render could fire an update check before HANDY_DISABLE_UPDATER is known.
  const loaded = !isLoading && settings !== null && updateChecksLocked !== null;
  // Forced-off by system configuration (HANDY_DISABLE_UPDATER) overrides the
  // stored preference without persisting it, mirroring the backend's effective
  // updater state.
  const enabled =
    (settings?.update_checks_enabled ?? false) && updateChecksLocked === false;
  return { loaded, enabled };
};

/** The words for where an update stands, shared by the footer and About. */
export const useUpdateStatusText = () => {
  const { t } = useTranslation();
  const { checking, installing, progress, upToDate, availableVersion } =
    useUpdateStore();
  if (installing) {
    return progress > 0 && progress < 100
      ? t("footer.downloading", { progress: progress.toString().padStart(3) })
      : progress === 100
        ? t("footer.installing")
        : t("footer.preparing");
  }
  if (checking) return t("footer.checkingUpdates");
  if (upToDate) return t("footer.upToDate");
  if (availableVersion) return t("footer.updateAvailableShort");
  return null;
};

/** The footer's update status. It also runs the checks: at launch, every few
 *  hours, and when the tray asks. */
const UpdateChecker: React.FC<UpdateCheckerProps> = ({ className = "" }) => {
  const { t } = useTranslation();
  const { loaded, enabled } = useUpdateChecksEnabled();
  const {
    checking,
    installing,
    progress,
    upToDate,
    availableVersion,
    portableDialog,
    portableInstallerUrl,
    check,
    install,
    reset,
    closePortableDialog,
  } = useUpdateStore();
  const status = useUpdateStatusText();

  useEffect(() => {
    if (!loaded) return;
    if (!enabled) {
      reset();
      return;
    }

    check();
    const interval = setInterval(check, CHECK_INTERVAL_MS);
    const unlistenCheck = listen("check-for-updates", () => check(true));
    const unlistenInstall = listen("install-update", () => install());

    return () => {
      clearInterval(interval);
      unlistenCheck.then((fn) => fn());
      unlistenInstall.then((fn) => fn());
    };
  }, [loaded, enabled]);

  const action = !enabled
    ? undefined
    : availableVersion && !installing
      ? install
      : !checking && !installing && !upToDate
        ? () => check(true)
        : undefined;

  // When no installer could be resolved for this target the button falls back to
  // the releases index, so the dialog has to say "browse" rather than "download".
  const hasDirectInstaller = portableInstallerUrl !== PORTABLE_RELEASES_URL;

  return (
    <>
      {portableDialog && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-background border border-line rounded-lg p-6 max-w-md w-full mx-4 space-y-4">
            <h2 className="text-base font-semibold">
              {t("footer.portableUpdateTitle")}
            </h2>
            <p className="text-sm text-ink-2">
              {hasDirectInstaller
                ? t("footer.portableUpdateMessage")
                : t("footer.portableUpdateBrowseMessage")}
            </p>
            <div className="flex gap-2 justify-end">
              <button
                className="px-3 py-1.5 text-sm rounded border border-line hover:bg-wash transition-colors"
                onClick={closePortableDialog}
              >
                {t("common.close")}
              </button>
              <button
                className="px-3 py-1.5 text-sm rounded bg-logo-primary text-white hover:bg-logo-primary/80 transition-colors"
                onClick={() => {
                  openUrl(portableInstallerUrl);
                  closePortableDialog();
                }}
              >
                {hasDirectInstaller
                  ? t("footer.portableUpdateButton")
                  : t("footer.portableUpdateBrowseButton")}
              </button>
            </div>
          </div>
        </div>
      )}
      <div className={`flex items-center gap-3 ${className}`}>
        {action ? (
          <button
            onClick={action}
            className={`transition-colors tabular-nums ${
              availableVersion
                ? "text-logo-primary hover:text-logo-primary font-medium"
                : "text-ink-2 hover:text-text"
            }`}
          >
            {status ?? t("footer.checkForUpdates")}
          </button>
        ) : (
          <span className="text-ink-2 tabular-nums">
            {enabled
              ? (status ?? t("footer.checkForUpdates"))
              : t("footer.updateCheckingDisabled")}
          </span>
        )}

        {installing && progress > 0 && progress < 100 && (
          <ProgressBar
            progress={[{ id: "update", percentage: progress }]}
            size="large"
          />
        )}
      </div>
    </>
  );
};

export default UpdateChecker;
