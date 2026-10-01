// One-command version bump. The version lives in four files and CI fails if
// they disagree (see docs/developer-guide/releasing.md + ci.yml "Check the
// version is consistent"), so this sets all four from one argument:
//
//   bun run bump 0.1.0-alpha.2
//
// Shields.io escapes a dash as a double dash inside a badge label, so
// `0.1.0-alpha.2` renders as `0.1.0--alpha.2` — same transform CI checks.
import { readFileSync, writeFileSync } from "node:fs";

const ver = process.argv[2];

if (ver === undefined || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(ver)) {
  console.error("usage: bun run bump <semver>  (e.g. 0.1.0-alpha.2)");
  process.exit(1);
}

function setJsonVersion(path) {
  const data = JSON.parse(readFileSync(path, "utf8"));
  data.version = ver;
  writeFileSync(path, `${JSON.stringify(data, null, 2)}\n`);
}

setJsonVersion("package.json");
setJsonVersion("src-tauri/tauri.conf.json");

const cargoPath = "src-tauri/Cargo.toml";
const cargo = readFileSync(cargoPath, "utf8").replace(
  /^version = ".*"/m,
  `version = "${ver}"`,
);
writeFileSync(cargoPath, cargo);

const readmePath = "README.md";
const badge = ver.replaceAll("-", "--");
const readme = readFileSync(readmePath, "utf8").replace(
  /badge\/version-(.+?)-blue/,
  `badge/version-${badge}-blue`,
);
writeFileSync(readmePath, readme);

console.log(`bumped to ${ver}: package.json, tauri.conf.json, Cargo.toml, README.md`);
