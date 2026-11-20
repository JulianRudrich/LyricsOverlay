// Sets the app's version everywhere the build reads it, on every system the
// CI runs on: node scripts/set-version.mjs 0.4.0-70
import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$/.test(version ?? "")) {
  console.error(`'${version}' is not a version like 0.4.0 or 0.4.0-70`);
  process.exit(1);
}

function edit(path, change) {
  const before = readFileSync(path, "utf8");
  const after = change(before);
  if (after === before) {
    console.error(`no version found in ${path}`);
    process.exit(1);
  }
  writeFileSync(path, after);
}

// The first `version = "…"` is the package's own.
edit("apps/desktop/src-tauri/Cargo.toml", (text) => text.replace(/^version = ".*"$/m, `version = "${version}"`));
for (const path of ["apps/desktop/package.json", "apps/desktop/src-tauri/tauri.conf.json"]) {
  edit(path, (text) => text.replace(/^  "version": ".*",$/m, `  "version": "${version}",`));
}
console.log(version);
