import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "fs";
import * as path from "path";
import { parseCheckJson, CheckJsonError, reach } from "../src/checkjson";
import { renderPanel } from "../src/panel";
import { projectTarget } from "../src/server";

const fx = (n: string) => fs.readFileSync(path.resolve(__dirname, "..", "..", "test", "fixtures", n), "utf8");

test("parses a recorded check --json for every example", () => {
  for (const n of fs.readdirSync(path.resolve(__dirname, "..", "..", "test", "fixtures")).filter((f) => f.startsWith("check-0"))) {
    const r = parseCheckJson(fx(n));
    assert.equal(r.schema_version, 1);
    assert.ok(r.units.length > 0, n);
  }
});

test("invoice-chaser panel names the agent and what it reaches", () => {
  const r = parseCheckJson(fx("check-01-invoice-chaser.json"));
  const agent = r.units.find((u) => u.kind === "agent")!;
  assert.deepEqual(
    reach(agent.effects).map((x) => x.name),
    ["model", ...agent.effects.tools.network, ...agent.effects.tools.local],
  );
  const html = renderPanel(r);
  assert.match(html, /invoice-chaser/);
  assert.match(html, /model, network\(crm, email\), local\(log\)/);
  assert.match(html, /Check passed/);
  assert.match(html, /chip network">crm</);
});

test("a failing project shows its errors and still renders", () => {
  const r = parseCheckJson(fx("check-error.json"));
  assert.equal(r.ok, false);
  const html = renderPanel(r);
  assert.match(html, /Check failed/);
  assert.match(html, /E205/);
  assert.match(html, /no effect summary is available/);
});

test("unverified bindings and rust blocks are called out; text is escaped", () => {
  const r = parseCheckJson(fx("check-05-invoice-service.json"));
  r.unverified_bindings.push({ name: "<b>sha2</b>", crate: "sha2", file: "bindings/sha2.agent.md", line: 5, proposed_effect: "pure", items: 1 });
  r.ok = false;
  const html = renderPanel(r);
  assert.match(html, /Needs your approval/);
  assert.match(html, /&lt;b&gt;sha2&lt;\/b&gt;/);
  assert.doesNotMatch(html, /<b>sha2/);
  assert.match(html, /Trusted Rust blocks/);
  assert.match(html, /fingerprint/);
});

test("rejects non-JSON and unknown schema versions", () => {
  assert.throws(() => parseCheckJson("error: nope"), CheckJsonError);
  assert.throws(() => parseCheckJson(JSON.stringify({ ok: true, schema_version: 2 })), /schema_version/);
});

test("projectTarget finds the folder holding agentmark.toml", () => {
  const exists = (p: string) => p === "/w/proj/agentmark.toml";
  assert.equal(projectTarget("/w/proj/agents/a.agent.md", exists), "/w/proj");
  assert.equal(projectTarget("/w/other/a.agent.md", exists), "/w/other/a.agent.md");
});

import { describeEffects } from "../src/panel";

test("plain-language wording for an operations manager", () => {
  const r = parseCheckJson(fx("check-01-invoice-chaser.json"));
  const agent = r.units.find((u) => u.kind === "agent")!;
  assert.deepEqual(describeEffects(agent.effects), [
    "can reach the network: crm, email",
    "asks the AI model",
    "writes local files or logs: log",
  ]);
  const pure = parseCheckJson(JSON.stringify({ schema_version: 1, ok: true, units: [{ name: "f", kind: "function", effects: { class: "pure", summary: "pure", tools: {} }, blocks: [] }] }));
  assert.deepEqual(describeEffects(pure.units[0].effects), ["pure: touches nothing outside itself"]);
});

test("strict CSP: nonce only, no remote sources", () => {
  const html = renderPanel(null, undefined, "abc123");
  assert.match(html, /default-src 'none'; style-src 'nonce-abc123'; script-src 'nonce-abc123'/);
  assert.doesNotMatch(html, /unsafe-inline|https?:\/\//);
  assert.equal((html.match(/<script nonce="abc123">/g) ?? []).length, 1);
  assert.match(html, /<style nonce="abc123">/);
});
