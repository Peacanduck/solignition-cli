#!/usr/bin/env node
// Solignition CLI npm wrapper.
//
// At install time, npm/yarn select exactly one of the platform packages
// listed in `optionalDependencies` based on the user's `os`/`cpu`. At run
// time, this wrapper resolves whichever one got installed and execs the
// shipped binary, forwarding argv and exit code.

const { spawnSync } = require("node:child_process");
const path = require("node:path");

// Map Node's process.platform / arch to the package name suffix used in
// `npm/binaries/<suffix>/package.json`.
const PLATFORM_MAP = {
  "linux-x64": "@solignition/cli-linux-x64",
  "linux-arm64": "@solignition/cli-linux-arm64",
  "darwin-x64": "@solignition/cli-darwin-x64",
  "darwin-arm64": "@solignition/cli-darwin-arm64",
  "win32-x64": "@solignition/cli-win32-x64",
};

const key = `${process.platform}-${process.arch}`;
const pkg = PLATFORM_MAP[key];

if (!pkg) {
  console.error(
    `solignition: unsupported platform ${key}. Supported: ${Object.keys(
      PLATFORM_MAP
    ).join(", ")}.`
  );
  console.error(
    "If you need this platform, please open an issue at https://github.com/ORG/solignition-cli/issues."
  );
  process.exit(1);
}

// Locate the platform package. require.resolve handles npm, yarn, pnpm,
// workspaces, and hoisted layouts uniformly.
let binaryPath;
try {
  const pkgJson = require.resolve(`${pkg}/package.json`);
  const pkgDir = path.dirname(pkgJson);
  const binName =
    process.platform === "win32" ? "solignition.exe" : "solignition";
  binaryPath = path.join(pkgDir, "bin", binName);
} catch (err) {
  console.error(
    `solignition: matching platform package "${pkg}" was not installed.`
  );
  console.error(
    "This usually means optional dependencies were skipped during install. Reinstall with optional dependencies enabled (e.g. `npm install --include=optional`)."
  );
  process.exit(1);
}

const result = spawnSync(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
});

if (result.error) {
  console.error(`solignition: failed to execute binary at ${binaryPath}`);
  console.error(result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 1);
