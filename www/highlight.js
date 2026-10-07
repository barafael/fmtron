// RON syntax highlighting for the online formatter. Pure functions only:
// also imported by the Node test suite (www/test/), which enforces the hard
// invariant the editor overlay depends on — the markup must contain exactly
// the input's characters, in order, or the caret drifts off the glyphs.

const KEYWORDS = new Set(['true', 'false', 'Some', 'None', 'inf', 'NaN']);

const NUMBER = /[-+]?(?:0[xob][0-9a-fA-F_]+|[0-9][0-9_]*(?:\.[0-9_]+)?(?:[eE][-+]?[0-9]+)?)(?:[iuf](?:8|16|32|64|128|size))?/y;
const IDENT = /[A-Za-z_][A-Za-z0-9_]*/y;
const RAW_STRING = /(?:br|b|r)(#*)"/y;
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

    // Attribute: #![...]
    if (c === '#' && src[i + 1] === '!') {
      const end = src.indexOf(']', i);
      const stop = end === -1 ? n : end + 1;
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

    // Byte or raw string: b"…"  r"…"  r#"…"#  br#"…"#
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

    // String literal.
    if (c === '"') {
      let j = i + 1;
      while (j < n) {
        if (src[j] === '\\') j += 2;
        else if (src[j] === '"') { j++; break; }
        else j++;
      }
      html += span('str', src.slice(i, j));
      i = j;
      continue;
    }

    // Char literal.
    if (c === "'") {
      let j = i + 1;
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
    if (ident && (/[A-Za-z_]/).test(c)) {
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
