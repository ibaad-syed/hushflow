import { create } from "zustand";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { arch, platform } from "@tauri-apps/plugin-os";
import { commands } from "@/bindings";
import {
  PORTABLE_RELEASES_URL,
  resolvePortableInstallerUrl,
} from "@/components/update-checker/portableInstaller";

/** How long "Up to date" shows after a check the user asked for. */
const UP_TO_DATE_MS = 3000;

// One update state for the footer, About and the tray, so they never disagree.
interface UpdateStore {
  checking: boolean;
  /** The newer release found, if any. */
  availableVersion: string | null;
  installing: boolean;
  /** Download progress while installing, 0–100. */
  progress: number;
  /** Briefly true after a check the user asked for found nothing. */
  upToDate: boolean;
  portableDialog: boolean;
  portableInstallerUrl: string;
  check: (manual?: boolean) => Promise<void>;
  install: () => Promise<void>;
  /** Forget a found update, e.g. when update checks are turned off. */
  reset: () => void;
  closePortableDialog: () => void;
}

let upToDateTimer: ReturnType<typeof setTimeout> | undefined;

const setAvailable = (version: string | null) => {
  useUpdateStore.setState({ availableVersion: version });
  commands.setAvailableUpdate(version).catch(() => {});
};

export const useUpdateStore = create<UpdateStore>()((set, get) => ({
  checking: false,
  availableVersion: null,
  installing: false,
  progress: 0,
  upToDate: false,
  portableDialog: false,
  portableInstallerUrl: PORTABLE_RELEASES_URL,

  check: async (manual = false) => {
    if (get().checking || get().installing) return;
    set({ checking: true, upToDate: false });
    try {
      const update = await check();
      if (update) {
        // Portable installs can't self-update in place — the manual dialog
        // links straight at the matching installer from this manifest instead.
        set({
          portableInstallerUrl: resolvePortableInstallerUrl(
            update.rawJson,
            platform(),
            arch(),
          ),
        });
        setAvailable(update.version);
      } else {
        setAvailable(null);
        if (manual) {
          set({ upToDate: true });
          clearTimeout(upToDateTimer);
          upToDateTimer = setTimeout(
            () => set({ upToDate: false }),
            UP_TO_DATE_MS,
          );
        }
      }
    } catch (error) {
      console.error("Failed to check for updates:", error);
    } finally {
      set({ checking: false });
    }
  },

  install: async () => {
    if (get().installing) return;
    if (await commands.isPortable()) {
      set({ portableDialog: true });
      return;
    }
    set({ installing: true, progress: 0 });
    try {
      const update = await check();
      if (!update) {
        setAvailable(null);
        return;
      }
      let downloaded = 0;
      let total = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          downloaded = 0;
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          const progress =
            total > 0 ? Math.round((downloaded / total) * 100) : 0;
          set({ progress: Math.min(progress, 100) });
        }
      });
      await relaunch();
    } catch (error) {
      console.error("Failed to install update:", error);
    } finally {
      set({ installing: false, progress: 0 });
    }
  },

  reset: () => {
    clearTimeout(upToDateTimer);
    set({ checking: false, upToDate: false });
    setAvailable(null);
  },

  closePortableDialog: () => set({ portableDialog: false }),
}));
