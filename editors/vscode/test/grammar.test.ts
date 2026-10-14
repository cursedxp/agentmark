import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "fs";
import * as path from "path";
import * as vsctm from "vscode-textmate";
import * as oniguruma from "vscode-oniguruma";

const root = path.resolve(__dirname, "..", "..");
const examples = path.resolve(root, "..", "..", "examples");

async function loadGrammar(): Promise<vsctm.IGrammar> {
  const wasm = fs.readFileSync(require.resolve("vscode-oniguruma/release/onig.wasm")).buffer;
  await oniguruma.loadWASM(wasm as ArrayBuffer);
  const registry = new vsctm.Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (s: string[]) => new oniguruma.OnigScanner(s),
      createOnigString: (s: string) => new oniguruma.OnigString(s),
    }),
    loadGrammar: async (scope) => {
      if (scope === "source.agentmark") {
        return vsctm.parseRawGrammar(
          fs.readFileSync(path.join(root, "syntaxes", "agentmark.tmLanguage.json"), "utf8"),
          "agentmark.tmLanguage.json",
        );
      }
      // VS Code supplies the real Rust grammar; an empty stand-in is enough to test the hand-off.
      if (scope === "source.rust") {
        return vsctm.parseRawGrammar(JSON.stringify({ scopeName: "source.rust", patterns: [] }), "rust.json");
      }
      return null;
    },
  });
  const g = await registry.loadGrammar("source.agentmark");
  assert.ok(g, "grammar loads");
  return g!;
}

function tokens(g: vsctm.IGrammar, text: string): { text: string; scopes: string[] }[][] {
  let state = vsctm.INITIAL;
  return text.split("\n").map((line) => {
    const r = g.tokenizeLine(line, state);
    state = r.ruleStack;
    return r.tokens.map((t) => ({ text: line.slice(t.startIndex, t.endIndex), scopes: t.scopes }));
  });
}

function has(line: { text: string; scopes: string[] }[], text: string, scopePart: string): boolean {
  return line.some((t) => t.text === text && t.scopes.some((s) => s.includes(scopePart)));
}

function agentFiles(dir: string): string[] {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name);
    return e.isDirectory() ? agentFiles(p) : e.name.endsWith(".agent.md") ? [p] : [];
  });
}

test("declaration headings, properties and comments", async () => {
  const g = await loadGrammar();
  const t = tokens(g, "> a note\n# Agent: invoice-chaser\n- model: claude\n# Import: crate serde_json as j");
  assert.ok(t[0].every((x) => x.scopes.includes("comment.line.greater-than.agentmark")));
  assert.ok(has(t[1], "Agent", "keyword.declaration"));
  assert.ok(has(t[1], "invoice-chaser", "entity.name.type"));
  assert.ok(has(t[2], "model", "variable.other.property"));
  assert.ok(has(t[3], "crate", "storage.modifier.crate"));
  assert.ok(has(t[3], "j", "entity.name.namespace"));
});

test("blocks, statements, guards and strings", async () => {
  const g = await loadGrammar();
  const src = [
    "## Every day at 09:00",
    "## When email receives message",
    "## Do remind",
    "- for each overdue invoice in crm.invoices:",
    '  - ask model with invoice: "Be polite {customer}" -> reminder',
    "  - guard: no amounts changed in reminder",
    '  - email send customer: "{reminder}"',
    "  - crm update invoice: status = \"reminded\"",
  ].join("\n");
  const t = tokens(g, src);
  assert.ok(has(t[0], "Every", "keyword.control.trigger"));
  assert.ok(has(t[0], "09:00", "constant.numeric.time"));
  assert.ok(has(t[1], "email", "support.class.tool"));
  assert.ok(has(t[2], "remind", "entity.name.function"));
  assert.ok(t[3].some((x) => x.scopes.some((s) => s.includes("keyword.control"))));
  assert.ok(has(t[4], "ask model", "keyword.other.model"));
  assert.ok(t[4].some((x) => x.scopes.some((s) => s.includes("string.quoted.double"))));
  assert.ok(has(t[5], "guard", "keyword.control.guard"));
  assert.ok(has(t[6], "email", "support.class.tool"));
  assert.ok(has(t[6], "send", "support.function.verb"));
  assert.ok(has(t[6], "customer", "entity.name.tag.target"));
});

test("a rust fence is handed to the rust grammar", async () => {
  const g = await loadGrammar();
  const t = tokens(g, "## Rust: h -> text\n- effect: pure\n  ```rust\n  let x = 1;\n  ```\n- after: 1");
  assert.ok(has(t[0], "Rust", "keyword.declaration"));
  assert.ok(t[3].every((x) => x.scopes.includes("meta.embedded.block.rust.agentmark")));
  assert.ok(!t[5].some((x) => x.scopes.includes("meta.embedded.block.rust.agentmark")), "fence closes");
});

test("every example file tokenizes, and every heading gets a specific scope", async () => {
  const g = await loadGrammar();
  const files = agentFiles(examples);
  assert.ok(files.length >= 10, `found ${files.length} example files`);
  for (const f of files) {
    const lines = tokens(g, fs.readFileSync(f, "utf8"));
    const src = fs.readFileSync(f, "utf8").split("\n");
    src.forEach((line, i) => {
      if (/^#{1,2}\s/.test(line)) {
        const generic = lines[i].some((t) => t.scopes.some((s) => s === "markup.heading.agentmark"));
        const specific = lines[i].some((t) =>
          t.scopes.some((s) => /keyword\.(declaration|control)|markup\.heading\.not-bound/.test(s)),
        );
        assert.ok(specific || !generic, `${path.basename(f)}:${i + 1} fell through to the generic heading: ${line}`);
      }
      if (/^\s*>/.test(line)) {
        assert.ok(
          lines[i].every((t) => t.scopes.includes("comment.line.greater-than.agentmark")),
          `${path.basename(f)}:${i + 1} comment`,
        );
      }
    });
  }
});

test("collection words (SPEC 22.2) get their own scope", async () => {
  const g = await loadGrammar();
  const src = [
    "  - set big = sales where amount > 5 EUR",
    "  - set ranked = sort sales by amount descending",
    '  - set regions = join (unique region in sales) with ", "',
    "  - set by_region = group sales by region",
    "  - set total = sum of amount in sales",
    '  - put 1 at "EU" in totals',
    '  - remove "US" from totals',
    "  - for each row in ranked with index i:",
    "  - for each region, rows in by_region:",
    "  - set names = keys of by_region",
    "  - set none = empty list of text",
  ].join("\n");
  const t = tokens(g, src);
  assert.ok(has(t[0], "where", "support.function.collection"));
  assert.ok(has(t[1], "sort", "support.function.collection"));
  assert.ok(has(t[1], "by", "support.function.collection"));
  assert.ok(has(t[1], "descending", "support.function.collection"));
  assert.ok(has(t[2], "unique", "support.function.collection"));
  assert.ok(has(t[2], "join", "support.function.collection"));
  assert.ok(has(t[3], "group", "support.function.collection"));
  assert.ok(has(t[4], "sum of", "support.function.collection"));
  assert.ok(has(t[5], "put", "keyword.control"));
  assert.ok(has(t[5], "at", "support.function.collection"));
  assert.ok(has(t[6], "remove", "keyword.control"));
  assert.ok(has(t[7], "index", "keyword.operator.word"));
  assert.ok(has(t[9], "keys of", "support.function.collection"));
  assert.ok(has(t[10], "empty list of", "support.function.collection"));
  // words inside a string stay string
  const s2 = tokens(g, '  - print: "sort the list by name"');
  assert.ok(!s2[0].some((x) => x.scopes.some((sc) => sc.includes("support.function.collection"))));
});
