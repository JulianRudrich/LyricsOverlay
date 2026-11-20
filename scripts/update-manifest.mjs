// Writes the latest.json the app's updater reads, from the signed update
// packages in a folder: node scripts/update-manifest.mjs <folder> <version> <download URL> [notes]
// Each package keeps one name from build to build, so its download address stays the same.
import { readFileSync } from "node:fs";
import { join } from "node:path";

const [folder, version, base, notes = ""] = process.argv.slice(2);
if (!folder || !version || !base) {
  console.error("usage: update-manifest.mjs <folder> <version> <download URL> [notes]");
  process.exit(1);
}

/** The updater's platform names, and the package each one installs. */
export const PACKAGES = {
  "darwin-aarch64": "Singline-macOS.app.tar.gz",
  "darwin-x86_64": "Singline-macOS.app.tar.gz",
  "windows-x86_64": "Singline-Windows-setup.exe",
  "linux-x86_64": "Singline-Linux.AppImage",
};

const platforms = Object.fromEntries(
  Object.entries(PACKAGES).map(([platform, file]) => [
    platform,
    { signature: readFileSync(join(folder, `${file}.sig`), "utf8").trim(), url: `${base}/${file}` },
  ]),
);

console.log(JSON.stringify({ version, notes, pub_date: new Date().toISOString(), platforms }, null, 2));
