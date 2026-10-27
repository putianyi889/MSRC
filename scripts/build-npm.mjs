import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = fileURLToPath(new URL("..", import.meta.url));
const output = path.join(root, "dist/npm");
const { name, version, repository, publishConfig } = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
const cargo = readFileSync(path.join(root, "Cargo.toml"), "utf8");
const cargoVersion = cargo.split("[workspace.package]")[1]?.split(/\n\[/)[0]?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (cargoVersion !== version) {
  throw new Error("Synchronize package.json and workspace.package.version in Cargo.toml");
}

for (const target of ["nodejs", "web"]) {
  const result = spawnSync("wasm-pack", [
    "build", "bindings/wasm", "--release", "--target", target,
    "--out-dir", path.join(output, target), "--out-name", "msrc", "--", "--locked"
  ], { cwd: root, stdio: "inherit", shell: false });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
  // wasm-pack writes a '*' .gitignore. npm otherwise uses that file and drops
  // every generated JS/WASM file, even when the parent manifest lists them.
  writeFileSync(path.join(output, target, ".npmignore"), "");
}

mkdirSync(output, { recursive: true });
for (const file of ["README.md", "LICENSE", "THIRD_PARTY_NOTICES.md"]) {
  copyFileSync(path.join(root, file), path.join(output, file));
}
writeFileSync(path.join(output, "package.json"), `${JSON.stringify({
  name,
  version,
  description: "Lossless, versioned compression for Minesweeper replay files",
  license: "MIT",
  repository,
  publishConfig,
  main: "./nodejs/msrc.js",
  types: "./nodejs/msrc.d.ts",
  exports: {
    ".": { types: "./nodejs/msrc.d.ts", default: "./nodejs/msrc.js" },
    "./web": { types: "./web/msrc.d.ts", default: "./web/msrc.js" },
    "./web/msrc_bg.wasm": "./web/msrc_bg.wasm"
  },
  files: ["nodejs", "web", "LICENSE", "README.md", "THIRD_PARTY_NOTICES.md"],
  engines: { node: ">=22" },
  keywords: ["minesweeper", "replay", "compression", "lzma", "wasm"]
}, null, 2)}\n`);
// Node's default endpoint is CommonJS. The /web endpoint is native ESM.
writeFileSync(path.join(output, "web/package.json"), '{"type":"module"}\n');
