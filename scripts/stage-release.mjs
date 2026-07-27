import {
  appendFileSync,
  copyFileSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, join } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const config = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const version = config.version;
const tag = process.env.RELEASE_TAG;

if (tag !== `v${version}`) {
  throw new Error(`release tag ${tag ?? "missing"} does not match v${version}`);
}

function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? files(path) : [path];
  });
}

function exactlyOne(candidates, label) {
  if (candidates.length !== 1) {
    throw new Error(`${label} expected 1 file, found ${candidates.length}`);
  }
  return candidates[0];
}

const bundleRoot = join(root, "src-tauri", "target", "aarch64-apple-darwin", "release", "bundle");
const artifacts = files(bundleRoot);
const updaterSource = exactlyOne(
  artifacts.filter((path) => path.endsWith(".app.tar.gz")),
  "updater artifact",
);
const signatureSource = `${updaterSource}.sig`;
const dmgSource = exactlyOne(
  artifacts.filter((path) => path.endsWith(".dmg")),
  "DMG artifact",
);
if (!artifacts.includes(signatureSource)) {
  throw new Error(`updater signature missing for ${basename(updaterSource)}`);
}
for (const [path, label] of [
  [updaterSource, "updater artifact"],
  [signatureSource, "updater signature"],
  [dmgSource, "DMG artifact"],
]) {
  if (statSync(path).size === 0) {
    throw new Error(`${label} is empty: ${basename(path)}`);
  }
}

const output = join(root, "release-assets");
mkdirSync(output, { recursive: true });
const updaterName = `Kxen_${version}_aarch64.app.tar.gz`;
const dmgName = `Kxen_${version}_aarch64.dmg`;
const updaterPath = join(output, updaterName);
const signaturePath = `${updaterPath}.sig`;
const dmgPath = join(output, dmgName);
copyFileSync(updaterSource, updaterPath);
copyFileSync(signatureSource, signaturePath);
copyFileSync(dmgSource, dmgPath);

const signature = readFileSync(signatureSource, "utf8").trim();
if (!signature) {
  throw new Error(`updater signature is empty: ${basename(signatureSource)}`);
}
const baseUrl = `https://releases.kxen.ai/${tag}`;
const manifestPath = join(output, "latest.json");
writeFileSync(
  manifestPath,
  `${JSON.stringify(
    {
      version,
      notes: `Kxen ${version} development preview`,
      pub_date: new Date().toISOString(),
      platforms: {
        "darwin-aarch64": {
          signature,
          url: `${baseUrl}/${updaterName}`,
        },
      },
    },
    null,
    2,
  )}\n`,
);

const notesPath = join(output, "release-notes.md");
writeFileSync(
  notesPath,
  `# Kxen ${version} development preview\n\nmacOS 14+ Apple Silicon signed and notarized build.\n`,
);

if (process.env.GITHUB_OUTPUT) {
  appendFileSync(
    process.env.GITHUB_OUTPUT,
    [
      `version=${version}`,
      `updater_path=${updaterPath}`,
      `signature_path=${signaturePath}`,
      `dmg_path=${dmgPath}`,
      `manifest_path=${manifestPath}`,
      `notes_path=${notesPath}`,
      `updater_name=${updaterName}`,
      `dmg_name=${dmgName}`,
      "",
    ].join("\n"),
  );
}
