import init, { format_ron } from './pkg/fmtron_wasm.js';

const EXAMPLES = [
  { name: 'Game config · tree-sitter-ron', file: 'examples/game-config.ron' },
  { name: 'Struct with maps · ron', file: 'examples/ron-example.ron' },
  { name: 'Animation graph · Bevy', file: 'examples/bevy-animgraph.ron' },
  { name: 'Editor config · zee', file: 'examples/zee-config.ron' },
  { name: 'Text adventure world · kingslayer', file: 'examples/kingslayer-world.ron' },
  { name: 'Misformatted: simple struct', file: 'examples/misformatted-simple.ron' },
  { name: 'Misformatted: comments', file: 'examples/misformatted-comments.ron' },
  { name: 'Misformatted: nested', file: 'examples/misformatted-nested.ron' },
];

const DEFAULT_EXAMPLE = EXAMPLES[0];

// --- RON syntax highlighting -------------------------------------------------

const KEYWORDS = new Set(['true', 'false', 'Some', 'None', 'inf', 'NaN']);

const NUMBER = /[-+]?(?:0[xob][0-9a-fA-F_]+|[0-9][0-9_]*(?:\.[0-9_]+)?(?:[eE][-+]?[0-9]+)?)(?:[iuf](?:8|16|32|64|128|size))?/y;
const IDENT = /[A-Za-z_][A-Za-z0-9_]*/y;

// Produces HTML with <span class="tok-*"> highlighting for RON source.
function highlightRon(src) {
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

    // Raw or byte string: r"…"  r#"…"#  br"…"  br#"…"#
    const rawStart = c === 'r' ? i : (c === 'b' && src[i + 1] === 'r' ? i + 1 : -1);
    if (rawStart !== -1 && (src[rawStart + 1] === '"' || src[rawStart + 1] === '#')) {
      let j = rawStart + 1;
      let hashes = 0;
      while (src[j] === '#') { hashes++; j++; }
      if (src[j] === '"') {
        const closer = '"' + '#'.repeat(hashes);
        const end = src.indexOf(closer, j + 1);
        const stop = end === -1 ? n : end + closer.length;
        html += span('str', src.slice(i, stop));
        i = stop;
        continue;
      }
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
      const after = src.slice(IDENT.lastIndex).match(/^[ \t\r\n]*/)[0].length;
      let cls;
      if (KEYWORDS.has(word)) cls = 'kw';
      else if (src[IDENT.lastIndex + after] === ':') cls = 'key';
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

function escapeHtml(s) {
  return s.replace(/[&<>]/g, ch => (ch === '&' ? '&amp;' : ch === '<' ? '&lt;' : '&gt;'));
}

// --- App state ---------------------------------------------------------------

const input = document.getElementById('input');
const highlightCode = document.getElementById('highlight-code');
const highlightPre = document.getElementById('highlight');
const output = document.getElementById('output');
const errorBox = document.getElementById('error');
const exampleSelect = document.getElementById('example');
const tabSize = document.getElementById('tab-size');
const maxWidth = document.getElementById('max-width');
const blankLines = document.getElementById('blank-lines');
const copyButton = document.getElementById('copy');

let formatTimer = null;

function formatNow() {
  const keep = blankLines.value === 'keep';
  try {
    const formatted = format_ron(input.value, Number(tabSize.value), Number(maxWidth.value), keep);
    output.innerHTML = highlightRon(formatted) + '\n';
    errorBox.hidden = true;
  } catch (e) {
    errorBox.textContent = String(e.message ?? e);
    errorBox.hidden = false;
  }
}

function formatSoon() {
  clearTimeout(formatTimer);
  formatTimer = setTimeout(formatNow, 150);
}

function loadExample(example) {
  fetch(example.file)
    .then(r => (r.ok ? r.text() : Promise.reject(new Error(`${r.status} ${r.statusText}`))))
    .then(text => {
      input.value = text;
      refreshHighlight();
      formatNow();
    })
    .catch(err => {
      input.value = '';
      errorBox.textContent = `Could not load example: ${err.message}`;
      errorBox.hidden = false;
    });
}

function refreshHighlight() {
  highlightCode.innerHTML = highlightRon(input.value) + '\n';
  syncScroll();
}

function syncScroll() {
  highlightPre.scrollTop = input.scrollTop;
  highlightPre.scrollLeft = input.scrollLeft;
}

input.addEventListener('input', () => {
  refreshHighlight();
  formatSoon();
});

input.addEventListener('scroll', syncScroll);

// Tab inserts indentation instead of leaving the textarea.
input.addEventListener('keydown', e => {
  if (e.key !== 'Tab') return;
  e.preventDefault();
  const { selectionStart: a, selectionEnd: b } = input;
  const spaces = ' '.repeat(Number(tabSize.value));
  input.setRangeText(spaces, a, b, 'end');
  refreshHighlight();
  formatSoon();
});

[tabSize, maxWidth, blankLines].forEach(el => el.addEventListener('change', formatNow));

copyButton.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText(output.textContent.replace(/\n$/, ''));
    copyButton.textContent = 'Copied!';
  } catch {
    copyButton.textContent = 'Copy failed';
  }
  setTimeout(() => { copyButton.textContent = 'Copy output'; }, 1500);
});

EXAMPLES.forEach((ex, idx) => {
  const option = document.createElement('option');
  option.value = String(idx);
  option.textContent = ex.name;
  exampleSelect.appendChild(option);
});

exampleSelect.addEventListener('change', () => loadExample(EXAMPLES[Number(exampleSelect.value)]));

init().then(() => {
  const initial = new URLSearchParams(location.search).get('example');
  const index = EXAMPLES.findIndex(ex => ex.file === initial);
  const start = index >= 0 ? index : EXAMPLES.indexOf(DEFAULT_EXAMPLE);
  exampleSelect.value = String(start);
  loadExample(EXAMPLES[start]);
});
