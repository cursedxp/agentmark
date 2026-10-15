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
  const t = tokens(g, "# Agent: invoice-chaser\n> a note\n- model: claude\n# Import: crate serde_json as j");
  assert.ok(has(t[0], "Agent", "keyword.declaration"));
  assert.ok(has(t[0], "invoice-chaser", "entity.name.type"));
  assert.ok(t[1].every((x) => x.scopes.includes("comment.line.greater-than.agentmark")));
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
    "1. for each overdue invoice in crm.invoices:",
    '   1. ask model with invoice: "Be polite {customer}" into reminder',
    "   2. guard: no amounts changed in reminder",
    '   3. email send customer: "{reminder}"',
    "   4. crm update invoice: status = \"reminded\"",
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

test("enums, type aliases and match branches", async () => {
  const g = await loadGrammar();
  const src = [
    "# Enum: Shape",
    "- Circle: radius: int",
    "# Type: Totals",
    "- is: list of int",
    "1. match shape:",
    "   - Circle c:",
    '     1. print: "{c.radius}"',
    "   - Shape.Empty:",
    "     1. stop",
    "   otherwise:",
    "   1. stop",
    "2. match found:",
    "   - some row:",
    "     1. stop",
    "   - none:",
    "     1. stop",
    "3. Circle { radius: 3 } into s",
    "4. Shape.Rect { width: 1, height: 2 } into e",
    "5. match parsed:",
    "   - ok:",
    "     1. return some 3",
    "   - error:",
    "     1. none into d",
  ].join("\n");
  const t = tokens(g, src);
  assert.ok(has(t[0], "Enum", "keyword.declaration"));
  assert.ok(has(t[0], "Shape", "entity.name.type"));
  assert.ok(has(t[2], "Type", "keyword.declaration"));
  assert.ok(has(t[3], "is", "variable.other.property"));
  assert.ok(has(t[4], "match", "keyword.control"));
  assert.ok(has(t[5], "Circle", "entity.name.type.variant"));
  assert.ok(has(t[5], "c", "variable.other"));
  assert.ok(has(t[7], "Shape.Empty", "entity.name.type.variant"));
  assert.ok(has(t[9], "otherwise", "keyword.control"), "otherwise is not a variant");
  assert.ok(!t[9].some((x) => x.scopes.some((s) => s.includes("variant"))));
  assert.ok(!t[9].some((x) => x.scopes.some((s) => s.includes("text.description"))), "otherwise: is a plain line, not a description");
  assert.ok(has(t[12], "some", "keyword.control.pattern"));
  assert.ok(has(t[14], "none", "keyword.control.pattern"));
  assert.ok(has(t[16], "Circle", "entity.name.type.variant"));
  assert.ok(has(t[17], "Shape", "entity.name.type.variant"));
  assert.ok(has(t[17], "Rect", "entity.name.type.variant"));
  assert.ok(has(t[19], "ok", "keyword.control.pattern"), "a branch without a name");
  assert.ok(has(t[20], "some", "keyword.control.pattern"));
  assert.ok(has(t[21], "error", "keyword.control.pattern"));
  assert.ok(has(t[22], "none", "constant.language"));
});

test("built-in library calls: module, function, and the file tool", async () => {
  const g = await loadGrammar();
  const t = tokens(g, [
    "1. text.split(line, \",\") into parts",
    "2. try csv.parse(content) as list of Sale into sales",
    "3. file rename path.join(a, b) to path.join(a, c)",
  ].join("\n"));
  assert.ok(has(t[0], "text", "support.class.builtin-module"));
  assert.ok(has(t[0], "split", "support.function.builtin"));
  assert.ok(has(t[1], "csv", "support.class.builtin-module"));
  assert.ok(has(t[1], "parse", "support.function.builtin"));
  assert.ok(has(t[2], "path", "support.class.builtin-module"));
  assert.ok(has(t[2], "join", "support.function.builtin"));
});

test("a rust fence is handed to the rust grammar", async () => {
  const g = await loadGrammar();
  const t = tokens(g, "## Rust: h -> text\n- touches: nothing\n  ```rust\n  let x = 1;\n  ```\n- after: 1");
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
    "  1. sales where amount > 5 EUR into big",
    "  2. sort sales by amount descending into ranked",
    '  3. join (unique region in sales) with ", " into regions',
    "  4. group sales by region into by_region",
    "  5. sum of amount in sales into total",
    '  6. put 1 at "EU" in totals',
    '  7. remove "US" from totals',
    "  8. for each row in ranked with index i:",
    "  9. for each region, rows in by_region:",
    "  10. keys of by_region into names",
    "  11. empty list of text into nothing_here",
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
  const s2 = tokens(g, '  1. print: "sort the list by name"');
  assert.ok(!s2[0].some((x) => x.scopes.some((sc) => sc.includes("support.function.collection"))));
});

test("handle types in a binding heading", async () => {
  const g = await loadGrammar();
  const t = tokens(g, "## update(self: mut handle of sha2.Sha256, data: bytes)\n## sha256_new() -> handle of sha2.Sha256");
  assert.ok(has(t[0], "update", "entity.name.function"));
  assert.ok(has(t[0], "self", "variable.parameter"));
  assert.ok(has(t[0], "mut", "storage.modifier.handle-mode"));
  assert.ok(has(t[0], "handle", "support.type"));
  assert.ok(has(t[0], "sha2.Sha256", "support.type.handle"));
  assert.ok(has(t[0], "data", "variable.parameter"));
  assert.ok(has(t[0], "bytes", "support.type"));
  assert.ok(has(t[1], "handle of sha2.Sha256", "support.type"));
});

test("a user-defined tool: heading, verb headings and the Rust fence of a verb", async () => {
  const g = await loadGrammar();
  const src = [
    "# Tool: slugger",
    "- touches: nothing",
    "",
    "## slug(title: text) -> text",
    "1. return regex.replace(title, \"[^a-z0-9]+\", \"-\")",
  ].join("\n");
  const t = tokens(g, src);
  assert.ok(has(t[0], "Tool", "keyword.declaration"));
  assert.ok(has(t[0], "slugger", "entity.name.type"));
  assert.ok(has(t[1], "touches", "variable.other.property"));
  assert.ok(has(t[3], "slug", "entity.name.function"));
  assert.ok(has(t[3], "->", "keyword.operator.arrow"));
});

test("v0.4: Program, Uses, descriptions, rule heading, steps, into and add", async () => {
  const g = await loadGrammar();
  const src = [
    "# Program: sales",
    "Reads a sales file and prints, for each region, what it brought in.",
    "",
    "- start: [sales-report](#command-sales-report)",
    "",
    "# Uses",
    "- [constants](../constants.agent.md)",
    "* [billing](./billing/) as billing",
    "",
    "# Rule: big Sale",
    "A sale of 100 EUR or more.",
    "",
    "- when: amount >= 100 EUR",
    "",
    "# Function: region_totals",
    "Adds up the sales.",
    "",
    "- in: sales",
    "- out: region totals",
    "- touches: nothing",
    "- steps:",
    "  1. group sales by region into by_region",
    "  2. for each region, region_sales in by_region:",
    "     1. add RegionTotal to totals:",
    "        - region",
    "        - orders: count of region_sales",
    "  3. if count of totals > 0:",
    "     1. stop",
    "     else:",
    "     1. stop",
    "  4. try file read path into sales",
    "     otherwise:",
    "     1. stop",
  ].join("\n");
  const t = tokens(g, src);
  assert.ok(has(t[0], "Program", "keyword.declaration"));
  assert.ok(has(t[0], "sales", "entity.name.type"));
  assert.ok(t[1].every((x) => x.scopes.includes("text.description.agentmark")), "description is plain text");
  assert.ok(!t[1].some((x) => x.scopes.some((s) => s.includes("keyword"))), "no keywords inside prose");
  assert.ok(has(t[3], "start", "variable.other.property"));
  assert.ok(has(t[3], "sales-report", "string.other.link.title"));
  assert.ok(has(t[3], "#command-sales-report", "markup.underline.link"));
  assert.ok(has(t[5], "Uses", "keyword.declaration"));
  assert.ok(has(t[6], "constants", "string.other.link.title"));
  assert.ok(has(t[6], "../constants.agent.md", "markup.underline.link"));
  assert.ok(has(t[7], "as", "keyword.control.as"));
  assert.ok(has(t[7], "billing", "entity.name.namespace"));
  assert.ok(has(t[9], "Rule", "keyword.declaration"));
  assert.ok(has(t[9], "Sale", "support.type.record"));
  assert.ok(t[10].every((x) => x.scopes.includes("text.description.agentmark")));
  assert.ok(has(t[18], "out", "variable.other.property"));
  assert.ok(has(t[19], "touches", "variable.other.property"));
  assert.ok(has(t[20], "steps", "variable.other.property"));
  assert.ok(has(t[21], "1.", "punctuation.definition.list"));
  assert.ok(has(t[21], "into", "keyword.operator.word"));
  assert.ok(has(t[22], "for each", "keyword.control"));
  assert.ok(has(t[23], "add", "keyword.control"));
  assert.ok(has(t[24], "-", "punctuation.definition.list"));
  assert.ok(has(t[28], "else", "keyword.control"));
  assert.ok(!t[28].some((x) => x.scopes.some((s) => s.includes("text.description"))), "else: is not a description");
  assert.ok(has(t[30], "try", "keyword.control"));
  assert.ok(has(t[31], "otherwise", "keyword.control"));
  assert.ok(!t[31].some((x) => x.scopes.some((s) => s.includes("text.description"))));
});

test("v0.4: a binding keeps - effect:, other parts do not", async () => {
  const g = await loadGrammar();
  const t = tokens(g, "# Binding: sha2\ngenerated by agentmark bind\n\n- crate: sha2\n- effect: pure\n\n## sha256_digest(data: bytes) -> bytes\nCompute hash of `data`.");
  assert.ok(has(t[0], "Binding", "keyword.declaration"));
  assert.ok(has(t[1], "generated by agentmark bind", "text.description"));
  assert.ok(has(t[3], "crate", "variable.other.property"));
  assert.ok(has(t[4], "effect", "variable.other.property"));
  assert.ok(has(t[6], "sha256_digest", "entity.name.function"));
  assert.ok(has(t[7], "Compute hash of `data`.", "text.description"));
  const u = tokens(g, "# Function: f\n- effect: pure");
  assert.ok(!has(u[1], "effect", "variable.other.property"), "effect: is no property outside a binding");
});

test("every description paragraph of every example is plain text", async () => {
  const g = await loadGrammar();
  for (const f of agentFiles(examples)) {
    const text = fs.readFileSync(f, "utf8");
    const lines = tokens(g, text);
    let fenced = false;
    text.split("\n").forEach((line, i) => {
      if (/^\s*```/.test(line)) {
        fenced = !fenced;
        return;
      }
      if (fenced || /^\s*(?:$|[-*+]\s|\d+\.(?:\s|$)|#|>)/.test(line) || /^\s*(?:else|otherwise):\s*$/.test(line)) return;
      if (/^\s*(?:`|\|)/.test(line)) return;
      assert.ok(
        lines[i].every((t) => t.scopes.includes("text.description.agentmark")),
        `${path.basename(f)}:${i + 1} is prose but not a description: ${line}`,
      );
    });
  }
});
