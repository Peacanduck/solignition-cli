#!/usr/bin/env node
// Stage the prebuilt binaries downloaded from a GitHub Release into the
// corresponding `npm/binaries/<platform>/bin/` directories, and rewrite
// each package's version to match the release tag.
//
// Invoked from .github/workflows/release.yml:
//   node npm/scripts/stage-binaries.mjs <dist-dir> <version>
//
// `<dist-dir>` is the directory containing the artifacts that
// `taiki-e/upload-rust-binary-action` produced — archives named
// `solignition-vX.Y.Z-<rust-target>.tar.gz` (or `.zip` for Windows).

import { execSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync, renameSync } from "node:fs";
import { join } from "node:path";

const [, , distDir, version] = process.argv;
if (!distDir || !version) {
  console.error("usage: stage-binaries.mjs <dist-dir> <version>");
  process.exit(1);
}

// Mapping: rust target triple → npm platform package directory.
const TARGETS = [
  { triple: "x86_64-unknown-linux-gnu", pkg: "linux-x64", bin: "solignition" },
  { triple: "aarch64-unknown-linux-gnu", pkg: "linux-arm64", bin: "solignition" },
  { triple: "x86_64-apple-darwin", pkg: "darwin-x64", bin: "solignition" },
  { triple: "aarch64-apple-darwin", pkg: "darwin-arm64", bin: "solignition" },
  { triple: "x86_64-pc-windows-msvc", pkg: "win32-x64", bin: "solignition.exe" },
];

for (const { triple, pkg, bin } of TARGETS) {
  const pkgDir = join("npm", "binaries", pkg);
  const binDir = join(pkgDir, "bin");
  mkdirSync(binDir, { recursive: true });

  const isWindows = triple.includes("windows");
  const archiveExt = isWindows ? "zip" : "tar.gz";
  const archive = join(distDir, `solignition-v${version}-${triple}.${archiveExt}`);

  // Extract the binary into bin/. We pipe through tar/unzip rather than
  // depending on an npm package — keeps the workflow self-contained.
  if (isWindows) {
    execSync(`unzip -o "${archive}" -d "${binDir}"`, { stdio: "inherit" });
  } else {
    execSync(`tar -xzf "${archive}" -C "${binDir}"`, { stdio: "inherit" });
  }

  // Some archive layouts put the binary inside a subdir matching the
  // archive name; flatten if needed.
  // (taiki-e/upload-rust-binary-action puts the binary at the archive
  // root by default, so this is usually a no-op.)

  // Bump version field to match the release tag.
  const pkgJsonPath = join(pkgDir, "package.json");
  const pkgJson = JSON.parse(readFileSync(pkgJsonPath, "utf8"));
  pkgJson.version = version;
  writeFileSync(pkgJsonPath, JSON.stringify(pkgJson, null, 2) + "\n");
}

// Bump the meta package's version + pin its optionalDependencies to the
// same version so they always resolve to the matching platform package.
const metaPath = join("npm", "cli", "package.json");
const meta = JSON.parse(readFileSync(metaPath, "utf8"));
meta.version = version;
for (const dep of Object.keys(meta.optionalDependencies ?? {})) {
  meta.optionalDependencies[dep] = version;
}
writeFileSync(metaPath, JSON.stringify(meta, null, 2) + "\n");

console.log(`Staged ${TARGETS.length} platform binaries at version ${version}.`);
