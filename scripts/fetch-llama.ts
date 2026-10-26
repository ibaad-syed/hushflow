// scripts/fetch-llama.ts — Download the pinned llama.cpp server for local cleanup
//
// The cleanup step runs a small LLM through llama.cpp's `llama-server`, started
// by the app as a child process (src-tauri/src/managers/cleanup). This script
// fetches the prebuilt server for the current platform from the pinned
// llama.cpp release into src-tauri/resources/llama/, which Tauri bundles.
// The directory is gitignored; run this once after cloning (and after bumping
// LLAMA_BUILD):
//
//   bun scripts/fetch-llama.ts
//
// Windows uses the Vulkan build so one binary covers NVIDIA, AMD and Intel GPUs.

import { execFileSync } from "child_process";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
  copyFileSync,
  writeFileSync,
  readFileSync,
} from "fs";
import { join, resolve } from "path";
import { tmpdir } from "os";

const LLAMA_BUILD = "b11887";

const root = resolve(import.meta.dirname, "..");
const outDir = join(root, "src-tauri", "resources", "llama");
const stamp = join(outDir, ".build");

const asset = (() => {
  const { platform, arch } = process;
  if (platform === "darwin" && arch === "arm64")
    return `llama-${LLAMA_BUILD}-bin-macos-arm64.tar.gz`;
  if (platform === "darwin") return `llama-${LLAMA_BUILD}-bin-macos-x64.tar.gz`;
  if (platform === "win32" && arch === "arm64")
    return `llama-${LLAMA_BUILD}-bin-win-vulkan-arm64.zip`;
  if (platform === "win32")
    return `llama-${LLAMA_BUILD}-bin-win-vulkan-x64.zip`;
  if (platform === "linux" && arch === "arm64")
    return `llama-${LLAMA_BUILD}-bin-ubuntu-vulkan-arm64.tar.gz`;
  return `llama-${LLAMA_BUILD}-bin-ubuntu-vulkan-x64.tar.gz`;
})();

// The server binary and the shared libraries it loads; the rest of the
// release (CLI tools, benchmarks) isn't needed.
// On macOS/Linux each library also appears under symlinked aliases; only the
// name the loader asks for (libfoo.0.dylib, libfoo.so.0) is kept.
const keep = (name: string) =>
  /^llama-server(\.exe)?$/.test(name) ||
  /^LICENSE/.test(name) ||
  /\.dll$/.test(name) ||
  /^lib[\w-]+\.0\.dylib$/.test(name) ||
  /^lib[\w-]+\.so\.0$/.test(name) ||
  /^libllama-server-impl\.(dylib|so)$/.test(name);

if (existsSync(stamp) && readFileSync(stamp, "utf8").trim() === asset) {
  console.log(`llama.cpp ${LLAMA_BUILD} already in ${outDir}`);
  process.exit(0);
}

const url = `https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA_BUILD}/${asset}`;
const work = join(tmpdir(), `llama-${LLAMA_BUILD}-${process.pid}`);
mkdirSync(work, { recursive: true });
const archive = join(work, asset);

console.log(`Downloading ${url}`);
const response = await fetch(url);
if (!response.ok) throw new Error(`Download failed: ${response.status} ${url}`);
await Bun.write(archive, response);

// bsdtar (macOS, Windows 10+) and GNU tar both read .tar.gz; bsdtar also reads .zip.
execFileSync("tar", ["-xf", archive, "-C", work], { stdio: "inherit" });

const findDir = (dir: string): string => {
  const entries = readdirSync(dir, { withFileTypes: true });
  if (entries.some((e) => e.isFile() && /^llama-server(\.exe)?$/.test(e.name)))
    return dir;
  for (const e of entries) {
    if (e.isDirectory()) {
      const found = findDir(join(dir, e.name));
      if (found) return found;
    }
  }
  return "";
};
const srcDir = findDir(work);
if (!srcDir) throw new Error("llama-server not found in the release archive");

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });
for (const name of readdirSync(srcDir).filter(keep)) {
  copyFileSync(join(srcDir, name), join(outDir, name));
}
writeFileSync(stamp, asset + "\n");
rmSync(work, { recursive: true, force: true });
console.log(`llama.cpp ${LLAMA_BUILD} installed in ${outDir}`);
