// Small AgentMark highlighter. Line oriented, ideas taken from
// editors/vscode/syntaxes/agentmark.tmLanguage.json. Returns HTML.
import hljs from 'highlight.js';

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const sp = (cls, s) => (s === '' ? '' : `<span class="${cls}">${esc(s)}</span>`);

const DECL = 'Agent|Record|Enum|Type|Rule|Function|Service|Command|Constant|Binding|Tool|Import|Run';
const STMT_KW = ['for each', 'if', 'else', 'otherwise', 'stop', 'do', 'set', 'return', 'respond', 'read', 'print',
  'match', 'append', 'put', 'remove', 'guard', 'try', 'parallel', 'ask model', 'some', 'ok', 'error', 'none'];
const WORD_KW = 'in|as|with|from|and|or|not|is|matches|else|index|handle|mut|consume|of';
const COLL = 'sort|group|take|skip|unique|join|where|by|descending|at|to|sum of|count of|list of|map of|keys of|values of|min of|max of|first of|last of|empty list of|empty map of';
const LIT = 'true|false|today|now|none';

const TOKEN = new RegExp([
  '(?<str>"(?:\\\\.|[^"\\\\])*")',
  '(?<date>\\b\\d{4}-\\d{2}-\\d{2}\\b)',
  '(?<time>\\b\\d{1,2}:\\d{2}\\b)',
  '(?<money>\\b\\d+(?:\\.\\d+)?\\s*[A-Z]{3}\\b)',
  '(?<dur>\\b\\d+\\s*(?:second|minute|hour|day|week|month|year)s?\\b)',
  '(?<num>\\b\\d+(?:\\.\\d+)?\\b)',
  `(?<coll>\\b(?:${COLL})\\b)`,
  `(?<lit>\\b(?:${LIT})\\b)`,
  `(?<kw>\\b(?:${WORD_KW})\\b)`,
  '(?<lib>\\b(?:text|regex|number|money|date|path|json|csv|file|command)(?=\\.[a-z_]+\\())',
  '(?<call>\\b[a-z_][\\w]*(?=\\())',
  '(?<type>\\b[A-Z][\\w-]*(?:\\.[A-Z][\\w-]*)?\\b)',
  '(?<arrow>->)',
  '(?<op><=|>=|!=|=|<|>|\\+|-|\\*|/)',
].join('|'), 'g');

function interp(inner) {
  // string body with {interpolation}
  return esc(inner).replace(/\{[^}"]*\}/g, (m) => `<span class="tk-interp">${m}</span>`)
    .replace(/\\./g, (m) => `<span class="tk-esc">${m}</span>`);
}

function expr(s) {
  let out = '';
  let last = 0;
  TOKEN.lastIndex = 0;
  for (const m of s.matchAll(TOKEN)) {
    out += esc(s.slice(last, m.index));
    const [name, val] = Object.entries(m.groups).find(([, v]) => v !== undefined);
    last = m.index + m[0].length;
    if (name === 'str') out += `<span class="tk-str">${interp(val)}</span>`;
    else if (name === 'date' || name === 'time' || name === 'money' || name === 'dur' || name === 'num') out += sp('tk-num', val);
    else if (name === 'coll') out += sp('tk-op2', val);
    else if (name === 'lit') out += sp('tk-lit', val);
    else if (name === 'kw') out += sp('tk-kw2', val);
    else if (name === 'lib' || name === 'call') out += sp('tk-fn', val);
    else if (name === 'type') out += sp('tk-type', val);
    else if (name === 'arrow') out += sp('tk-arrow', val);
    else out += sp('tk-punct', val);
  }
  return out + esc(s.slice(last));
}

const PROPS = /^(\s*)(-)(\s+)(model|tools|memory|timeout|for|when|value|in|out|effect|listen|state|args|crate|doc|is)(:)(.*)$/;
const KW_LINE = new RegExp(`^(${STMT_KW.map((k) => k.replace(' ', '\\s')).join('|')})\\b(.*)$`);

function statement(rest) {
  // rest: text after "- "
  let m = rest.match(KW_LINE);
  if (m) {
    return sp('tk-kw', m[1]) + expr(m[2]);
  }
  // pattern or field line: "name: type" / "Name:" / tool call "file read path -> x"
  m = rest.match(/^([A-Za-z_][\w-]*)(:)(.*)$/);
  if (m) return sp('tk-field', m[1]) + sp('tk-punct', m[2]) + expr(m[3]);
  m = rest.match(/^((?:[a-z][\w-]*)(?:\.[a-z][\w-]*)*(?:\s+[a-z][\w-]*){0,2})(?=\s|$|:|")(.*)$/);
  if (m && !/^(is|and|or|not)$/.test(m[1])) {
    // leading lowercase words = tool call; trailing args get normal expression colouring
    const words = m[1].split(/\s+/);
    const head = words.length > 2 ? words.slice(0, 2).join(' ') : m[1];
    return sp('tk-fn', head) + expr(rest.slice(head.length));
  }
  return expr(rest);
}

function line(l) {
  if (/^\s*>/.test(l)) return sp('tk-com', l);
  let m = l.match(new RegExp(`^(#)(\\s+)(${DECL})(:)?(\\s*)(.*)$`));
  if (m) {
    const [, h, s1, kind, colon, s2, name] = m;
    let tail;
    if (kind === 'Import') {
      const im = name.match(/^(crate\s+)?(\S+)(?:(\s+)(as)(\s+)(\S+))?$/);
      tail = im ? `${im[1] ? sp('tk-kw2', im[1]) : ''}${sp('tk-str', im[2] || '')}${im[4] ? esc(im[3]) + sp('tk-kw2', im[4]) + esc(im[5]) + sp('tk-type', im[6]) : ''}` : esc(name);
    } else tail = sp('tk-type', name);
    return sp('tk-hash', h) + esc(s1) + sp('tk-decl', kind) + (colon ? sp('tk-punct', colon) : '') + esc(s2) + tail;
  }
  m = l.match(/^(##)(\s+)(Every|When|On start|Run|Do|Not bound|Rust|GET|POST|PUT|PATCH|DELETE)\b(.*)$/);
  if (m) {
    return sp('tk-hash', m[1]) + esc(m[2]) + sp('tk-trigger', m[3]) + expr(m[4]).replace(/^(:)/, '$1');
  }
  m = l.match(/^(##)(\s+)([A-Za-z_]\w*)(\()([^)]*)(\))(.*)$/);
  if (m) return sp('tk-hash', m[1]) + esc(m[2]) + sp('tk-fn', m[3]) + sp('tk-punct', m[4]) + expr(m[5]) + sp('tk-punct', m[6]) + expr(m[7]);
  m = l.match(/^(#{1,6})(\s+)(.*)$/);
  if (m) return sp('tk-hash', m[1]) + esc(m[2]) + sp('tk-decl', m[3]);
  m = l.match(PROPS);
  if (m) return esc(m[1]) + sp('tk-punct', m[2]) + esc(m[3]) + sp('tk-prop', m[4]) + sp('tk-punct', m[5]) + expr(m[6]);
  m = l.match(/^(\s*)(-)(\s+)(.*)$/);
  if (m) return esc(m[1]) + sp('tk-punct', m[2]) + esc(m[3]) + statement(m[4]);
  return expr(l);
}

export function highlightAgentMark(code) {
  const lines = code.split('\n');
  const out = [];
  for (let i = 0; i < lines.length; i++) {
    const f = lines[i].match(/^(\s*)(`{3,})rust\b.*$/);
    if (f) {
      const close = new RegExp('^\\s*`{' + f[2].length + ',}\\s*$');
      const body = [];
      let j = i + 1;
      while (j < lines.length && !close.test(lines[j])) body.push(lines[j++]);
      out.push(sp('tk-punct', lines[i]));
      if (body.length) out.push(hljs.highlight(body.join('\n'), { language: 'rust' }).value);
      if (j < lines.length) out.push(sp('tk-punct', lines[j]));
      i = j;
      continue;
    }
    out.push(line(lines[i]));
  }
  return out.join('\n');
}
