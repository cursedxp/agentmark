import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "fs";
import * as path from "path";

const root = path.resolve(__dirname, "..", "..");
const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));

test("every file the manifest points at exists", () => {
  const c = pkg.contributes;
  const files = [
    c.languages[0].configuration,
    c.grammars[0].path,
    c.jsonValidation[0].url,
    "LICENSE",
    "README.md",
  ];
  for (const f of files) assert.ok(fs.existsSync(path.join(root, f)), f);
  assert.ok(pkg.main.endsWith("extension.js"));
});

test("commands declared in package.json are the ones activate registers", () => {
  const src = fs.readFileSync(path.join(root, "src", "extension.ts"), "utf8");
  for (const cmd of pkg.contributes.commands) assert.ok(src.includes(`"${cmd.command}"`), cmd.command);
});

test(".agent.md is registered as a language extension", () => {
  assert.deepEqual(pkg.contributes.languages[0].extensions, [".agent.md"]);
});
