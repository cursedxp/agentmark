// Renders README.md + docs/*.md into ONE self-contained HTML page (artifact content only).
import { readFileSync, writeFileSync, mkdirSync, existsSync, statSync } from 'node:fs';
import { dirname, resolve, relative, join, posix } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Marked } from 'marked';
import hljs from 'highlight.js';
import { highlightAgentMark } from './agentmark-lang.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const GH = 'https://github.com/cursedxp/agentmark';

const argv = process.argv.slice(2);
const oi = argv.indexOf('--out');
const out = resolve(oi >= 0 ? argv[oi + 1] : join(here, 'dist/agentmark-docs.html'));

const DOCS = [
  { id: 'overview', file: 'README.md', title: 'Overview' },
  { id: 'guide', file: 'docs/guide.md', title: 'Guide' },
  { id: 'reference', file: 'docs/reference.md', title: 'Reference' },
  { id: 'libraries', file: 'docs/libraries.md', title: 'Libraries' },
  { id: 'editors', file: 'docs/editor.md', title: 'Editors' },
  { id: 'internals', file: 'docs/internals.md', title: 'Internals' },
  { id: 'check-json', file: 'docs/check-json.md', title: 'check --json' },
];
const byFile = new Map(DOCS.map((d) => [d.file, d]));

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const decode = (s) => s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&');
const ghSlug = (t) => t.toLowerCase().trim().replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s/g, '-');

// ---- pass 1: collect heading anchors so cross-doc links resolve --------------
function headingText(raw) {
  return raw.replace(/`([^`]*)`/g, '$1').replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/[*_]/g, (c) => (c === '_' ? c : ''));
}
const anchors = new Map(); // docId -> Set of github slugs
for (const d of DOCS) {
  d.src = readFileSync(join(root, d.file), 'utf8');
  const set = new Set();
  const seen = new Map();
  let fence = null;
  for (const l of d.src.split('\n')) {
    const f = l.match(/^\s*(`{3,}|~{3,})/);
    if (f) { if (!fence) fence = f[1]; else if (l.trim().startsWith(fence)) fence = null; continue; }
    if (fence) continue;
    const m = l.match(/^(#{1,6})\s+(.*?)\s*#*\s*$/);
    if (!m) continue;
    let s = ghSlug(headingText(m[2]));
    const n = seen.get(s) || 0;
    seen.set(s, n + 1);
    set.add(n ? `${s}-${n}` : s);
  }
  anchors.set(d.id, set);
}

function resolveHref(href, doc) {
  if (/^(https?:|mailto:)/.test(href)) return { href, external: true };
  if (href.startsWith('#')) {
    const a = href.slice(1);
    return { href: anchors.get(doc.id).has(a) ? `#${doc.id}--${a}` : `#${doc.id}` };
  }
  const [p, frag] = href.split('#');
  const rel = posix.normalize(posix.join(posix.dirname(doc.file), p));
  const target = byFile.get(rel);
  if (target) {
    if (frag && anchors.get(target.id).has(frag)) return { href: `#${target.id}--${frag}` };
    return { href: `#${target.id}` };
  }
  const abs = join(root, rel);
  const isDir = existsSync(abs) && statSync(abs).isDirectory();
  return { href: `${GH}/${isDir ? 'tree' : 'blob'}/main/${rel}${frag ? '#' + frag : ''}`, external: true };
}

// ---- code blocks -------------------------------------------------------------
function parseInfo(info) {
  const parts = (info || '').trim().split(/\s+/).filter(Boolean);
  const lang = parts.shift() || '';
  const o = { lang, caption: '', tags: [] };
  for (const p of parts) {
    const kv = p.match(/^([\w-]+)=(.*)$/);
    if (kv) {
      if (kv[1] === 'file' || kv[1] === 'name') o.caption = o.caption || kv[2].replace(/^["']|["']$/g, '');
      else if (kv[1] === 'says') o.says = kv[2];
    } else if (p === 'invalid') o.tags.push('does not compile');
    else if (p === 'stderr') o.tags.push('stderr');
    else if (p === 'fragment' || p === 'http') { /* modifier only */ }
    else if (/[./]/.test(p)) o.caption = o.caption || p;
  }
  return o;
}
const LANG = { sh: 'bash', console: 'shell', rust: 'rust', toml: 'ini', json: 'json', lua: 'lua', bash: 'bash', shell: 'shell' };
const warned = new Set();
function renderCode(text, info) {
  const o = parseInfo(info);
  let body;
  if (o.lang === 'agentmark') body = highlightAgentMark(text);
  else if (LANG[o.lang]) body = hljs.highlight(text, { language: LANG[o.lang] }).value;
  else { body = esc(text); if (o.lang && !warned.has(o.lang)) { warned.add(o.lang); console.warn('plain language:', o.lang); } }
  const cap = [o.caption && `<span class="cap-name">${esc(o.caption)}</span>`, ...o.tags.map((t) => `<span class="cap-tag">${esc(t)}</span>`)]
    .filter(Boolean).join('');
  const label = o.lang === 'agentmark' ? 'agentmark' : o.lang;
  return `<figure class="code${o.tags.includes('does not compile') ? ' is-invalid' : ''}"><figcaption><span class="cap-lang">${esc(label)}</span>${cap}<button type="button" class="copy" aria-label="Copy code">Copy</button></figcaption><pre tabindex="0"><code>${body.replace(/\n$/, '')}</code></pre></figure>`;
}

// ---- render ------------------------------------------------------------------
const nav = [];
function renderDoc(doc) {
  const seen = new Map();
  const entry = { id: doc.id, title: doc.title, items: [] };
  nav.push(entry);
  const m = new Marked({ gfm: true });
  m.use({
    renderer: {
      code({ text, lang }) { return renderCode(text, lang); },
      heading({ tokens, depth }) {
        const inner = this.parser.parseInline(tokens);
        const plain = decode(inner.replace(/<[^>]+>/g, ''));
        const s = ghSlug(plain);
        const n = seen.get(s) || 0;
        seen.set(s, n + 1);
        const slug = n ? `${s}-${n}` : s;
        const id = depth === 1 ? doc.id : `${doc.id}--${slug}`;
        if (depth === 2 || depth === 3) entry.items.push({ id, depth, text: plain });
        return `<h${depth} id="${esc(id)}">${inner}<a class="anchor" href="#${esc(id)}" aria-label="Link to this heading">#</a></h${depth}>\n`;
      },
      link({ href, title, tokens }) {
        const r = resolveHref(href, doc);
        const t = title ? ` title="${esc(title)}"` : '';
        return `<a href="${esc(r.href)}"${t}${r.external ? ' target="_blank" rel="noopener noreferrer"' : ''}>${this.parser.parseInline(tokens)}</a>`;
      },
      table(token) {
        const head = token.header.map((c, i) => `<th${token.align[i] ? ` style="text-align:${token.align[i]}"` : ''}>${this.parser.parseInline(c.tokens)}</th>`).join('');
        const rows = token.rows.map((r) => `<tr>${r.map((c, i) => `<td${token.align[i] ? ` style="text-align:${token.align[i]}"` : ''}>${this.parser.parseInline(c.tokens)}</td>`).join('')}</tr>`).join('');
        return `<div class="tablewrap" tabindex="0"><table><thead><tr>${head}</tr></thead><tbody>${rows}</tbody></table></div>\n`;
      },
      html({ text }) { return esc(text); },
    },
  });
  const html = m.parse(doc.src);
  return `<section class="doc" id="sec-${doc.id}" data-doc="${doc.id}">\n${html}</section>`;
}
const sections = DOCS.map(renderDoc).join('\n');

const navHtml = nav.map((d) => {
  const items = d.items.map((i) => `<li class="d${i.depth}"><a href="#${esc(i.id)}" data-text="${esc(i.text.toLowerCase())}">${esc(i.text)}</a></li>`).join('');
  return `<div class="ng" data-doc="${d.id}"><div class="ng-head"><button type="button" class="ng-toggle" aria-expanded="false" aria-label="Toggle ${esc(d.title)} headings"></button><a class="ng-link" href="#${d.id}">${esc(d.title)}</a></div><ul>${items}</ul></div>`;
}).join('\n');

const css = readFileSync(join(here, 'style.css'), 'utf8');
const js = readFileSync(join(here, 'client.js'), 'utf8');
const page = `<title>AgentMark Docs</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Newsreader:ital,opsz,wght@0,6..72,400;0,6..72,600;1,6..72,400&family=Public+Sans:wght@400;500;600&family=JetBrains+Mono:wght@400;500&display=swap">
<style>
${css}
</style>
<a class="skip" href="#main">Skip to content</a>
<header class="bar">
  <button type="button" class="menu" id="menu" aria-expanded="false" aria-controls="side">Contents</button>
  <span class="bar-title">AgentMark Docs</span>
  <button type="button" class="theme" aria-label="Switch colour theme">Theme</button>
</header>
<div class="shell">
  <aside class="side" id="side" aria-label="Documentation navigation">
    <div class="side-in">
      <div class="brand">AgentMark<span>documentation</span></div>
      <button type="button" class="theme side-theme" aria-label="Switch colour theme">Theme</button>
      <label class="search"><span class="sr">Filter headings</span><input id="q" type="search" placeholder="Filter headings" autocomplete="off" spellcheck="false"></label>
      <nav id="nav">
${navHtml}
      </nav>
      <p class="none" id="none" hidden>No heading matches.</p>
    </div>
  </aside>
  <div class="scrim" id="scrim"></div>
  <main class="main" id="main">
    <article class="prose">
${sections}
    </article>
  </main>
</div>
<script>
${js}
</script>
`;
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, page);
console.log(`wrote ${out} (${(Buffer.byteLength(page) / 1024).toFixed(0)} KB)`);
