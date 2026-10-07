import init, { format_ron } from './pkg/fmtron_wasm.js';
import { highlightRon } from './highlight.js';

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

let formatTimer = null;

// A number input can hold a typed value outside its min/max, or nothing at
// all (valueAsNumber is then NaN; `value` would be '' and read as 0). Every
// read of a control goes through one clamp to the control's own range, so
// the formatter, the tab rendering and the inserted indentation always agree
// with each other and with what the browser marks as valid.
function clampInt(el, fallback) {
  const v = Math.floor(el.valueAsNumber);
  if (!Number.isFinite(v)) return fallback;
  return Math.min(Math.max(v, Number(el.min)), Number(el.max));
}

const readTabSize = () => clampInt(tabSize, 4);
const readMaxWidth = () => clampInt(maxWidth, 100);

function showError(message) {
  errorBox.textContent = message;
  errorBox.classList.remove('busy');
  errorBox.hidden = false;
}

function formatNow() {
  const keep = blankLines.value === 'keep';
  try {
    const formatted = format_ron(input.value, readTabSize(), readMaxWidth(), keep);
    output.innerHTML = highlightRon(formatted) + '\n';
    errorBox.hidden = true;
  } catch (e) {
    showError(String(e.message ?? e));
  }
}

function formatSoon() {
  clearTimeout(formatTimer);
  formatTimer = setTimeout(formatNow, 150);
}

// Says where the selected example comes from. Built from DOM nodes, not
// markup, so the manifest strings never need escaping.
function showAttribution(example) {
  attribution.replaceChildren();
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
      showAttribution(example);
      input.value = '';
      refreshHighlight();
      showError(`Could not load example: ${err.message}`);
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

// Keep literal tab characters in the input aligned with the option, and with
// fmtron's own indentation.
function applyTabSize() {
  document.documentElement.style.setProperty('--tab', readTabSize());
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
errorBox.textContent = 'Loading the formatter…';
errorBox.classList.add('busy');
errorBox.hidden = false;
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
