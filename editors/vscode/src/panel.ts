// Renders the "what can this program touch" panel as HTML. Pure: takes a report, returns a string.
import { randomBytes } from "crypto";
import { CheckReport, Effects, reach, Unit } from "./checkjson";

export function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function link(file: string, line: number, text?: string): string {
  const label = text ?? `${file}:${line}`;
  return `<a href="#" data-file="${esc(file)}" data-line="${esc(String(line))}">${esc(label)}</a>`;
}

/** What a part touches, as `check` and `doc` word it: `touches AI, crm, email, log` or `touches nothing`. */
export function touchesLine(e: Effects): string {
  const names = new Set<string>();
  for (const n of [...e.tools.network, ...e.tools.local, ...e.tools.unverified]) names.add(n === "file" ? "files" : n);
  const rest = [...names].sort();
  const all = e.tools.model ? ["AI", ...rest] : rest;
  return all.length === 0 ? "touches nothing" : `touches ${all.join(", ")}`;
}

/** A Rust block's declared class, said the way `check` says it. */
function saysTouches(effect: string): string {
  const w: Record<string, string> = {
    pure: "nothing",
    local: "this machine",
    network: "outside systems",
    model: "AI",
  };
  return `says it touches ${w[effect] ?? effect}`;
}

/** Plain-language lines for an operations manager, from one effect summary. */
export function describeEffects(e: Effects): string[] {
  const out = [touchesLine(e)];
  if (e.tools.unverified.length) out.push(`uses libraries nobody approved yet: ${e.tools.unverified.join(", ")}`);
  return out;
}

function plain(e: Effects): string {
  return `<ul class="plain">${describeEffects(e).map((l) => `<li>${esc(l)}</li>`).join("")}</ul>`;
}

function reachList(e: Effects): string {
  const r = reach(e);
  if (r.length === 0) return `<span class="muted">nothing</span>`;
  return r.map((x) => `<span class="chip ${esc(String(x.class))}">${esc(x.name === "model" ? "AI" : x.name === "file" ? "files" : x.name)}</span>`).join(" ");
}

function unitSection(u: Unit): string {
  const blocks = u.blocks
    .map(
      (b) =>
        `<tr><td>${link(b.file, b.line, b.label)}</td><td>${reachList(b.effects)}</td></tr>`,
    )
    .join("");
  return `<section class="unit">
  <h2>${esc(u.name)} <span class="kind">${esc(String(u.kind))}</span></h2>
  ${plain(u.effects)}
  <p>${reachList(u.effects)}</p>
  ${blocks ? `<table><thead><tr><th>Block</th><th>Touches</th></tr></thead><tbody>${blocks}</tbody></table>` : ""}
</section>`;
}

export function renderPanel(report: CheckReport | null, error?: string, nonce: string = randomBytes(16).toString("base64")): string {
  const body: string[] = [];
  if (error) {
    body.push(`<div class="banner error" role="alert">${esc(error)}</div>`);
  }
  if (report) {
    const errs = report.diagnostics.filter((d) => d.severity === "error");
    const warns = report.diagnostics.filter((d) => d.severity === "warning");
    body.push(
      `<div class="banner ${report.ok ? "ok" : "error"}">${
        report.ok ? "Check passed" : "Check failed"
      } for ${esc(report.path)} &middot; ${esc(String(errs.length))} error(s), ${esc(String(warns.length))} warning(s)</div>`,
    );
    if (report.units.length === 0 && !report.ok) {
      body.push(`<p class="muted">The project has errors, so no summary of what it touches is available yet.</p>`);
    }
    body.push(...report.units.map(unitSection));

    if (report.unverified_bindings.length) {
      body.push(
        `<section><h2>Needs your approval</h2><p>These libraries have a binding whose <code>- effect:</code> line nobody approved (a library binding keeps that line).</p><ul>${report.unverified_bindings
          .map(
            (b) =>
              `<li>${link(b.file, b.line, b.name)} (crate <code>${esc(b.crate)}</code>, ${esc(String(b.items))} item(s)) proposed effect: ${esc(
                b.proposed_effect ?? "none",
              )}</li>`,
          )
          .join("")}</ul></section>`,
      );
    }
    if (report.rust_blocks.length) {
      body.push(
        `<section><h2>Trusted Rust blocks</h2><p>The compiler cannot verify these; what they say they touch (<code>- touches:</code>) is taken on trust.</p><ul>${report.rust_blocks
          .map(
            (r) =>
              `<li>${link(r.file, r.line, r.name)}${r.parent ? ` in ${esc(r.parent)}` : ""}: ${esc(saysTouches(r.effect))}</li>`,
          )
          .join("")}</ul></section>`,
      );
    }
    if (report.diagnostics.length) {
      body.push(
        `<section><h2>Problems</h2><ul>${report.diagnostics
          .map(
            (d) =>
              `<li class="${esc(String(d.severity))}">${link(d.file, d.line)} <strong>${esc(String(d.code))}</strong> ${esc(d.message)}${
                d.did_you_mean ? ` Did you mean <code>${esc(d.did_you_mean)}</code>?` : ""
              }${d.help ? `<div class="help">${esc(d.help)}</div>` : ""}</li>`,
          )
          .join("")}</ul></section>`,
      );
    }
  }
  return `<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
<title>What can this program touch</title>
<style nonce="${nonce}">
body{font-family:var(--vscode-font-family);color:var(--vscode-foreground);padding:12px 16px}
h2{font-size:1.1em;margin:1.2em 0 .3em}
.kind{font-weight:normal;color:var(--vscode-descriptionForeground);font-size:.85em}
.chip{border-radius:10px;padding:1px 8px;font-size:.85em;border:1px solid var(--vscode-panel-border)}
.chip.local{background:rgba(60,160,90,.25)}
.chip.network{background:rgba(220,150,40,.3)}.chip.model{background:rgba(150,90,220,.3)}
.chip.unverified{background:rgba(220,60,60,.35)}
.banner{padding:8px 12px;border-radius:4px;margin-bottom:8px}
.banner.ok{background:rgba(60,160,90,.2)}.banner.error{background:rgba(220,60,60,.2)}
table{border-collapse:collapse;width:100%}td,th{text-align:left;padding:3px 8px;border-bottom:1px solid var(--vscode-panel-border)}
.muted,.help{color:var(--vscode-descriptionForeground)}
li.error strong{color:var(--vscode-errorForeground)}
a{color:var(--vscode-textLink-foreground)}
</style></head><body>
<h1>What can this program touch</h1>
${body.join("\n")}
<script nonce="${nonce}">
const vscode = typeof acquireVsCodeApi === "function" ? acquireVsCodeApi() : null;
document.addEventListener("click", (e) => {
  const a = e.target.closest && e.target.closest("a[data-file]");
  if (!a) return;
  e.preventDefault();
  if (vscode) vscode.postMessage({ file: a.dataset.file, line: Number(a.dataset.line) });
});
</script>
</body></html>`;
}
