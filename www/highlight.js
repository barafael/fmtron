// RON syntax highlighting for the online formatter. Pure functions only:
// also imported by the Node test suite (www/test/), which enforces the hard
// invariant the editor overlay depends on — the markup must contain exactly
// the input's characters, in order, or the caret drifts off the glyphs.

const KEYWORDS = new Set(['true', 'false', 'Some', 'None', 'inf', 'NaN']);

// The exponent is the one place RON allows a leading underscore (`1e_3`).
const NUMBER = /[-+]?(?:0[xob][0-9a-fA-F_]+|(?:[0-9][0-9_]*(?:\.[0-9_]*)?|\.[0-9_]+)(?:[eE][-+]?[0-9_]+)?)(?:[iuf](?:8|16|32|64|128|size))?/y;
// Identifiers are Unicode, as in Rust; a raw one (`r#type`, `r#a.b-c`) may
// also hold `.`, `+` and `-`.
const IDENT = /r#[\p{ID_Continue}.+-]+|[\p{ID_Start}_]\p{ID_Continue}*/uy;
const IDENT_START = /[\p{ID_Start}_]/u;
const RAW_STRING = /b?r(#*)"/y;
// Sticky, applied at the index right after an identifier: a run of
// whitespace and then a colon marks the identifier as a struct key.
const KEY_COLON = /[ \t\r\n]*:/y;

// Produces HTML with <span class="tok-*"> highlighting for RON source.
export function highlightRon(src) {
  let html = '';
  let i = 0;
  const n = src.length;

  const span = (cls, text) => `<span class="tok-${cls}">${escapeHtml(text)}</span>`;

  while (i < n) {
    const c = src[i];

    // Attribute: #![...], to the `]` outside its strings.
    if (c === '#' && src[i + 1] === '!') {
      let j = i + 2;
      while (j < n && src[j] !== ']') {
        if (src[j] === '"') {
          j++;
          while (j < n && src[j] !== '"') j += src[j] === '\\' ? 2 : 1;
        }
        j++;
      }
      const stop = Math.min(j + 1, n);
      html += span('attr', src.slice(i, stop));
      i = stop;
      continue;
    }

    // Line comment.
    if (c === '/' && src[i + 1] === '/') {
      let j = src.indexOf('\n', i);
      if (j === -1) j = n;
      html += span('com', src.slice(i, j));
      i = j;
      continue;
    }

    // Block comment, possibly nested.
    if (c === '/' && src[i + 1] === '*') {
      let depth = 0;
      let j = i;
      while (j < n) {
        if (src[j] === '/' && src[j + 1] === '*') { depth++; j += 2; }
        else if (src[j] === '*' && src[j + 1] === '/') { depth--; j += 2; if (depth === 0) break; }
        else j++;
      }
      html += span('com', src.slice(i, j));
      i = j;
      continue;
    }

    // Raw string: r"…"  r#"…"#  br#"…"#. No escapes: it ends at the first
    // quote followed by as many hashes as it opened with.
    RAW_STRING.lastIndex = i;
    const raw = RAW_STRING.exec(src);
    if (raw) {
      const closer = '"' + '#'.repeat(raw[1].length);
      const end = src.indexOf(closer, i + raw[0].length);
      const stop = end === -1 ? n : end + closer.length;
      html += span('str', src.slice(i, stop));
      i = stop;
      continue;
    }

    // String or byte string: "…" b"…", with backslash escapes.
    const byte = c === 'b' ? 1 : 0;
    const q = src[i + byte];
    if (q === '"') {
      let j = i + byte + 1;
      while (j < n) {
        if (src[j] === '\\') j += 2;
        else if (src[j] === '"') { j++; break; }
        else j++;
      }
      html += span('str', src.slice(i, j));
      i = j;
      continue;
    }

    // Char or byte char: '…' b'…'.
    if (q === "'") {
      let j = i + byte + 1;
      if (src[j] === '\\') {
        j += 2;
        while (j < n && src[j] !== "'") j++;
      } else if (j < n) {
        j += src.codePointAt(j) >= 0x10000 ? 2 : 1;
      }
      if (src[j] === "'") {
        html += span('chr', src.slice(i, j + 1));
        i = j + 1;
        continue;
      }
    }

    // Number.
    NUMBER.lastIndex = i;
    const num = NUMBER.exec(src);
    if (num && (c === '-' || c === '+' || c === '.' || (c >= '0' && c <= '9'))) {
      html += span('num', num[0]);
      i = NUMBER.lastIndex;
      continue;
    }

    // Identifier or keyword.
    IDENT.lastIndex = i;
    const ident = IDENT.exec(src);
    if (ident && (IDENT_START.test(c) || (c === 'r' && src[i + 1] === '#'))) {
      const word = ident[0];
      KEY_COLON.lastIndex = IDENT.lastIndex;
      let cls;
      if (KEYWORDS.has(word)) cls = 'kw';
      else if (KEY_COLON.exec(src)) cls = 'key';
      else if (/^[A-Z]/.test(word)) cls = 'type';
      else cls = 'id';
      html += span(cls, word);
      i = IDENT.lastIndex;
      continue;
    }

    // Punctuation and anything else.
    if ('()[]{}:,'.includes(c)) {
      html += span('punc', c);
      i++;
    } else {
      html += escapeHtml(c);
      i++;
    }
  }
  return html;
}

export function escapeHtml(s) {
  return s.replace(/[&<>]/g, ch => (ch === '&' ? '&amp;' : ch === '<' ? '&lt;' : '&gt;'));
}
