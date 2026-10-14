import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "fs";
import * as path from "path";
import Ajv from "ajv";
import { parse } from "smol-toml";

const root = path.resolve(__dirname, "..", "..");
const examples = path.resolve(root, "..", "..", "examples");
const schema = JSON.parse(fs.readFileSync(path.join(root, "schemas", "agentmark.schema.json"), "utf8"));
const ajv = new Ajv({ allErrors: true, strict: false });
const validate = ajv.compile(schema);
const ok = (toml: string) => {
  const good = validate(parse(toml));
  assert.ok(good, JSON.stringify(validate.errors));
};
const bad = (toml: string) => assert.equal(validate(parse(toml)), false, toml);

test("every example's agentmark.toml is valid", () => {
  const dirs = fs.readdirSync(examples).filter((d) => fs.existsSync(path.join(examples, d, "agentmark.toml")));
  assert.ok(dirs.length >= 1);
  for (const d of dirs) ok(fs.readFileSync(path.join(examples, d, "agentmark.toml"), "utf8"));
});

test("[models] maps environments to driver strings, [models.local] holds base_url", () => {
  ok(`[models]
production = "claude"
test = "mock"
dev = "local:kat-coder"
staging = "openai:gpt-4o"

[models.local]
base_url = "http://localhost:11434/v1"
`);
  bad(`[models]\nproduction = "gpt-4"\n`);
  bad(`[models]\nproduction = 3\n`);
  bad(`[models.local]\nbase = "x"\n`);
});

test("rust dependencies, lsp and lint tables", () => {
  ok(`[rust.dependencies]\nserde_json = "1"\nreqwest = { version = "0.12" }\n[lsp]\nmodel_actions = true\n[lint]\ndeny = ["missing-doc"]\n`);
  bad(`[lint]\ndeny = ["no-such-rule"]\n`);
  bad(`[lsp]\nmodel_action = true\n`);
  bad(`unknown_key = 1\n`);
  bad(`targets = ["java"]\n`);
});
