import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const packageRoot = path.resolve(process.env.MSRC_NPM_PACKAGE ?? fileURLToPath(new URL("../dist/npm", import.meta.url)));
const require = createRequire(path.join(packageRoot, "package.json"));
const packageName = "@putianyi888/msrc";
const msrc = require(packageName);
const original = Buffer.concat([Buffer.from("MSRC interoperability fixture\n"), ...Array.from({ length: 4 }, () => Buffer.from(Array.from({ length: 256 }, (_, i) => i)))]);
const hex = readFileSync(new URL("./fixtures/lzma-v1.hex", import.meta.url), "utf8");
const fixture = Buffer.from(hex.replaceAll(/\s/g, ""), "hex");

function checkApi(api) {
  for (const data of [Buffer.alloc(0), Buffer.from([0, 255]), original]) {
    const archive = api.compress(data);
    assert.ok(archive instanceof Uint8Array);
    assert.deepEqual(Buffer.from(api.decompress(archive)), data);
    const info = api.inspect(archive);
    try {
      assert.equal(info.original_size, BigInt(data.length));
      assert.equal(info.container_version, 1);
      assert.equal(info.algorithm_id, 1);
      assert.equal(info.algorithm_version, 1);
    } finally {
      info.free();
    }
  }
  assert.deepEqual(Buffer.from(api.decompress(fixture)), original);
  assert.throws(() => api.decompress(fixture, original.length - 1));
  assert.throws(() => api.decompress(fixture, undefined, 0));
  for (const level of [-1, 10, 1.5, NaN, Infinity]) {
    assert.throws(() => api.compress(original, level));
  }
  for (const limit of [-1, 1.5, NaN, Infinity, 2 ** 32]) {
    assert.throws(() => api.decompress(fixture, limit));
    assert.throws(() => api.decompress(fixture, undefined, limit));
  }
  assert.throws(() => api.decompress(fixture.subarray(0, fixture.length - 1)));
}

test("Node.js WASM API and independent LZMA fixture", () => checkApi(msrc));

test("web ESM API, initialized with bytes (no browser or server)", async () => {
  const web = await import(pathToFileURL(require.resolve(`${packageName}/web`)));
  await web.default({ module_or_path: readFileSync(require.resolve(`${packageName}/web/msrc_bg.wasm`)) });
  checkApi(web);
});
