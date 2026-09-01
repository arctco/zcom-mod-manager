import { readFileSync } from "node:fs";

const tag = process.argv[2];
if (!tag) {
  console.error("Usage: npm run check:release-version -- v<version>");
  process.exit(2);
}

const expected = tag.replace(/^v/, "");
if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(expected)) {
  console.error(`Invalid release tag: ${tag}`);
  process.exit(2);
}

const readJson = path => JSON.parse(readFileSync(path, "utf8"));
const versions = new Map([
  ["package.json", readJson("package.json").version],
  ["package-lock.json", readJson("package-lock.json").version],
  ["package-lock.json root package", readJson("package-lock.json").packages[""].version],
  ["src-tauri/tauri.conf.json", readJson("src-tauri/tauri.conf.json").version],
]);

for (const path of ["src-tauri/Cargo.toml", "src-tauri/Cargo.lock"]) {
  const contents = readFileSync(path, "utf8");
  const match = contents.match(
    path.endsWith("Cargo.toml")
      ? /^\[package\]\s+[\s\S]*?^version\s*=\s*"([^"]+)"/m
      : /^name\s*=\s*"zcom-mod-manager"\s+version\s*=\s*"([^"]+)"/m,
  );
  versions.set(path, match?.[1]);
}

const mismatches = [...versions].filter(([, version]) => version !== expected);
if (mismatches.length > 0) {
  console.error(`Release tag ${tag} does not match the project version:`);
  for (const [path, version] of mismatches) {
    console.error(`  ${path}: ${version ?? "version not found"}`);
  }
  process.exit(1);
}

console.log(`All project versions match ${tag}.`);
