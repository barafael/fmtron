import init, { FormatFailure, format_ron } from './pkg/fmtron_wasm.js';
import { highlightRon } from './highlight.js';
import { parseRon, renderStructure } from './structure.js';

// Real-world files are copied unchanged from fmtron's test corpus; `source`
// is where each one lives and `license` what its project is licensed under
// (as recorded in test_data/corpus/manifest.ron).
const EXAMPLES = [
  {
    name: 'Game config · tree-sitter-ron',
    file: 'examples/game-config.ron',
    source: 'tree-sitter-grammars/tree-sitter-ron · examples/game-config.ron',
    url: 'https://github.com/tree-sitter-grammars/tree-sitter-ron/blob/master/examples/game-config.ron',
    license: 'Apache-2.0',
  },
  {
    name: 'Struct with maps · ron',
    file: 'examples/ron-example.ron',
    source: 'ron-rs/ron · examples/example.ron',
    url: 'https://github.com/ron-rs/ron/blob/master/examples/example.ron',
    license: 'Apache-2.0',
  },
  {
    name: 'Animation graph · Bevy',
    file: 'examples/bevy-animgraph.ron',
    source: 'bevyengine/bevy · assets/animation_graphs/Fox.animgraph.ron',
    url: 'https://github.com/bevyengine/bevy/blob/main/assets/animation_graphs/Fox.animgraph.ron',
    license: 'Apache-2.0',
  },
  {
    name: 'Editor config · zee',
    file: 'examples/zee-config.ron',
    source: 'tree-sitter-grammars/tree-sitter-ron · examples/zee-config.ron',
    url: 'https://github.com/tree-sitter-grammars/tree-sitter-ron/blob/master/examples/zee-config.ron',
    license: 'Apache-2.0',
  },
  {
    name: 'Text adventure world · kingslayer',
    file: 'examples/kingslayer-world.ron',
    source: 'Zaechus/kingslayer · src/world.ron',
    url: 'https://github.com/Zaechus/kingslayer/blob/main/src/world.ron',
    license: 'MIT',
  },
  { name: 'Misformatted: simple struct', file: 'examples/misformatted-simple.ron' },
  { name: 'Misformatted: comments', file: 'examples/misformatted-comments.ron' },
  { name: 'Misformatted: nested', file: 'examples/misformatted-nested.ron' },
];

const DEFAULT_EXAMPLE = EXAMPLES[0];

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
const attribution = document.getElementById('attribution');
const errorMark = document.getElementById('error-mark');
const outputPane = document.getElementById('output-pane');
const structurePane = document.getElementById('structure-pane');
const structure = document.getElementById('structure');
const viewButtons = [...document.querySelectorAll('.views [role="tab"]')];

// 'formatted' or 'structure'. The structural view is built only while it is
// showing; `structureStale` remembers that the text changed meanwhile.
let view = 'formatted';
let structureStale = true;

// Where the current parse error is, as 1-based line and column, or null.
let errorAt = null;

let formatTimer = null;

// Above this many characters the editor shows plain text: highlighting runs
// on every keystroke, and at this size it (and the HTML it produces) costs
// more than a frame. The formatter itself is fine with far more.
const HIGHLIGHT_LIMIT = 128 * 1024;
const editor = input.closest('.editor');

// A number input can hold a typed value outside its min/max, or nothing at
// all (valueAsNumber is then NaN; `value` would be '' and read as 0). Every
// read of a control goes through one clamp to the control's own range, so
// the formatter, the tab rendering and the inserted indentation always agree
// with each other and with what the browser marks as valid.
function clampInt(el, fallback) {
  const v = Math.floor(el.valueAsNumber);
  if (!Number.isFinite(v)) return fallback;
  // A missing attribute means no bound, not a bound of 0.
  const lo = el.min === '' ? -Infinity : Number(el.min);
  const hi = el.max === '' ? Infinity : Number(el.max);
  return Math.min(Math.max(v, lo), hi);
}

const readTabSize = () => clampInt(tabSize, 4);
const readMaxWidth = () => clampInt(maxWidth, 100);

function showError(message) {
  errorBox.textContent = message;
  errorBox.classList.remove('note');
  errorBox.hidden = false;
}

// Something worth knowing that is not an error.
function showNote(message) {
  errorBox.textContent = message;
  errorBox.classList.add('note');
  errorBox.hidden = false;
}

// A lone surrogate (half of a UTF-16 pair, only ever pasted) has no UTF-8
// form; the formatter would receive U+FFFD in its place and the output would
// differ from the input there.
function hasLoneSurrogate(text) {
  return typeof text.isWellFormed === 'function' && !text.isWellFormed();
}

function showOutput(text) {
  if (text.length > HIGHLIGHT_LIMIT) {
    output.textContent = text + '\n';
  } else {
    output.innerHTML = highlightRon(text) + '\n';
  }
  structureStale = true;
  if (view === 'structure') showStructure();
}

// Lays the input out as nested shapes. The input has just been formatted,
// so it parses; should the page's own parser still disagree, it says so
// rather than drawing a wrong tree.
function showStructure() {
  if (!structureStale) return;
  structureStale = false;
  structure.replaceChildren();
  try {
    structure.append(renderStructure(parseRon(input.value), document));
  } catch (e) {
    const note = document.createElement('p');
    note.className = 'legend';
    note.textContent = `No structure to show: ${e.message}`;
    structure.append(note);
  }
}

function selectView(name) {
  view = name;
  for (const b of viewButtons) b.setAttribute('aria-selected', String(b.id === `view-${name}`));
  outputPane.hidden = name !== 'formatted';
  structurePane.hidden = name !== 'structure';
  if (name === 'structure') showStructure();
}

for (const b of viewButtons) {
  b.addEventListener('click', () => selectView(b.id.replace('view-', '')));
}

// Paints the band behind the erroring line of the input, if there is one,
// where the textarea currently scrolls it.
function placeErrorMark() {
  if (!errorAt) {
    errorMark.hidden = true;
    return;
  }
  const lineHeight = parseFloat(getComputedStyle(input).lineHeight);
  const paddingTop = parseFloat(getComputedStyle(input).paddingTop);
  errorMark.style.top = `${paddingTop + (errorAt.line - 1) * lineHeight - input.scrollTop}px`;
  errorMark.style.height = `${lineHeight}px`;
  errorMark.hidden = false;
}

function setErrorAt(at) {
  errorAt = at;
  errorBox.classList.toggle('clickable', Boolean(at));
  placeErrorMark();
}

// The UTF-16 index of a 1-based line and column (in characters, as the
// formatter counts them).
function indexOf(text, line, column) {
  let i = 0;
  for (let l = 1; l < line; l++) {
    const next = text.indexOf('\n', i);
    if (next === -1) return text.length;
    i = next + 1;
  }
  for (let c = 1; c < column && i < text.length; c++) {
    i += text.codePointAt(i) >= 0x10000 ? 2 : 1;
  }
  return i;
}

function formatNow() {
  const keep = blankLines.value === 'keep';
  const text = input.value;
  try {
    showOutput(format_ron(text, readTabSize(), readMaxWidth(), keep));
    setErrorAt(null);
    if (hasLoneSurrogate(text)) {
      showNote('The input holds an unpaired surrogate character, which the output shows as \u{FFFD}.');
    } else {
      errorBox.hidden = true;
    }
  } catch (e) {
    structureStale = true;
    if (view === 'structure') {
      structure.replaceChildren();
      structureStale = false;
    }
    if (e instanceof FormatFailure) {
      showError(e.rendered);
      setErrorAt(e.line > 0 ? { line: e.line, column: e.column } : null);
      e.free();
    } else {
      showError(String(e.message ?? e));
      setErrorAt(null);
    }
  }
}

// A click on a parse error puts the caret where it is.
errorBox.addEventListener('click', () => {
  if (!errorAt) return;
  const at = indexOf(input.value, errorAt.line, errorAt.column);
  input.focus();
  input.setSelectionRange(at, at);
  // Scroll the line into view: the textarea follows its caret on input, not
  // on selection changes, so nudge it by hand.
  const lineHeight = parseFloat(getComputedStyle(input).lineHeight);
  const top = (errorAt.line - 1) * lineHeight;
  if (top < input.scrollTop || top > input.scrollTop + input.clientHeight - lineHeight) {
    input.scrollTop = Math.max(0, top - input.clientHeight / 2);
  }
});

function formatSoon() {
  clearTimeout(formatTimer);
  formatTimer = setTimeout(formatNow, 150);
}

// Says where the selected example comes from. Built from DOM nodes, not
// markup, so the manifest strings never need escaping.
function showAttribution(example) {
  attribution.replaceChildren();
  if (!example) {
    attribution.textContent = 'No example loaded.';
    return;
  }
  if (!example.source) {
    attribution.textContent = 'This example was written for the demo to show fmtron at work.';
    return;
  }
  const link = document.createElement('a');
  link.href = example.url;
  link.textContent = example.source;
  attribution.append('Example from ', link, ` (${example.license}), unchanged from the original.`);
}

// Only the most recent selection may land: a slower earlier fetch must not
// overwrite a later one, or what the user typed meanwhile.
let latestLoad = 0;

function loadExample(example) {
  const load = ++latestLoad;
  fetch(example.file)
    .then(r => (r.ok ? r.text() : Promise.reject(new Error(`${r.status} ${r.statusText}`))))
    .then(text => {
      if (load !== latestLoad) return;
      showAttribution(example);
      input.value = text;
      refreshHighlight();
      formatNow();
    })
    .catch(err => {
      if (load !== latestLoad) return;
      // Nothing is loaded: say so in all three places.
      showAttribution(null);
      input.value = '';
      refreshHighlight();
      output.textContent = '';
      showError(`Could not load example: ${err.message}`);
    });
}

function refreshHighlight() {
  const text = input.value;
  const plain = text.length > HIGHLIGHT_LIMIT;
  editor.classList.toggle('plain', plain);
  highlightCode.innerHTML = plain ? '' : highlightRon(text) + '\n';
  syncScroll();
}

function syncScroll() {
  highlightPre.scrollTop = input.scrollTop;
  highlightPre.scrollLeft = input.scrollLeft;
  placeErrorMark();
}

// Keep literal tab characters in the input aligned with the option, and with
// fmtron's own indentation.
function applyTabSize() {
  document.documentElement.style.setProperty('--tab', readTabSize());
}

input.addEventListener('input', () => {
  // The text is the user's now: an example still loading must not replace it.
  latestLoad++;
  refreshHighlight();
  formatSoon();
});

input.addEventListener('scroll', syncScroll);

// Tab inserts indentation instead of leaving the textarea. So that the
// keyboard is not trapped in it, Escape lets the next Tab move focus on, as
// the hint under the heading says.
let tabLeaves = false;
input.addEventListener('blur', () => { tabLeaves = false; });
input.addEventListener('keydown', e => {
  if (e.key === 'Escape') {
    tabLeaves = true;
    return;
  }
  if (e.key !== 'Tab') {
    tabLeaves = false;
    return;
  }
  if (tabLeaves) {
    tabLeaves = false;
    return;
  }
  e.preventDefault();
  const spaces = ' '.repeat(readTabSize());
  // insertText keeps the edit on the textarea's native undo stack
  // (setRangeText does not) and fires `input` itself, which refreshes the
  // highlight and schedules a format. Fall back where it is unavailable.
  const inserted = document.execCommand('insertText', false, spaces);
  if (!inserted) {
    const { selectionStart: a, selectionEnd: b } = input;
    input.setRangeText(spaces, a, b, 'end');
    refreshHighlight();
    formatSoon();
  }
});

tabSize.addEventListener('input', applyTabSize);
tabSize.addEventListener('change', formatNow);
[maxWidth, blankLines].forEach(el => el.addEventListener('change', formatNow));

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

// The formatter module loads asynchronously; say so instead of presenting an
// empty editor that looks broken on slow connections.
showNote('Loading the formatter…');
applyTabSize();

init()
  .then(() => {
    const initial = new URLSearchParams(location.search).get('example');
    const index = EXAMPLES.findIndex(ex => ex.file === initial);
    const start = index >= 0 ? index : EXAMPLES.indexOf(DEFAULT_EXAMPLE);
    exampleSelect.value = String(start);
    loadExample(EXAMPLES[start]);
  })
  .catch(e => {
    showError(`Failed to load the formatter: ${e.message ?? e}`);
  });
