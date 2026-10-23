import { highlightRon } from '../highlight.js';

const list = document.getElementById('list');
const q = document.getElementById('q');
const filter = document.getElementById('filter');
const count = document.getElementById('count');
const detailTitle = document.getElementById('detail-title');
const detailMeta = document.getElementById('detail-meta');
const settingsEl = document.getElementById('settings');
const sources = document.getElementById('sources');
const rQ = document.getElementById('r-q');
const onlyFailed = document.getElementById('only-failed');
const showInput = document.getElementById('show-input');
const resultsEl = document.getElementById('results');

let index = null;
let current = null;
let cache = new Map();
let inputsCache = new Map();

async function loadIndex() {
  const res = await fetch('./data/index.json');
  index = await res.json();
  renderList();
}

function matches(entry, term, f) {
  if (!term && f === 'all') return true;
  const hay = (entry.id + ' ' + entry.settings + ' ' + (entry.file || '') + ' ' + (entry.configs || []).join(' ')).toLowerCase();
  if (term && !hay.includes(term)) return false;
  if (f === 'failed') return entry.failed_count > 0;
  if (f === 'static') return !entry.id.startsWith('generated-');
  if (f === 'generated') return entry.id.startsWith('generated-');
  return true;
}

function renderList() {
  if (!index) return;
  const term = (q.value || '').toLowerCase().trim();
  const f = filter.value;
  list.replaceChildren();
  let shown = 0;
  for (const entry of index.entries) {
    if (!matches(entry, term, f)) continue;
    shown += 1;
    const btn = document.createElement('button');
    btn.className = 'row';
    btn.dataset.id = entry.id;
    const title = document.createElement('div');
    title.className = 'row-title';
    title.textContent = entry.id;
    const meta = document.createElement('div');
    meta.className = 'row-meta';
    meta.textContent = `${entry.inputs_count} inputs · ${entry.failed_count} failed`;
    btn.appendChild(title);
    btn.appendChild(meta);
    btn.addEventListener('click', () => select(entry));
    list.appendChild(btn);
  }
  count.textContent = `${shown} / ${index.count} rows`;
}

q.addEventListener('input', renderList);
filter.addEventListener('change', renderList);

async function loadMatrix(id) {
  if (cache.has(id)) return cache.get(id);
  const res = await fetch(`./data/${id}.ron`);
  const text = await res.text();
  // Parse our RON shape by line, not by first delimiter: a `]` in a config
  // file name or a `)` in a settings value must not end the block early.
  const mSettings = text.match(/\n    settings: \(([\s\S]*?)\n    \),/);
  const configs = [];
  const mcfg = text.match(/\n    configs: \[([\s\S]*?)\n    \],/);
  if (mcfg) {
    const re2 = /"((?:[^"\\]|\\.)*)"/g;
    let m2;
    while ((m2 = re2.exec(mcfg[1])) !== null) configs.push(m2[1]);
  }
  const results = [];
  // `result` is always `Formatted("...")` or `Failed("...")`. Match the RON
  // string literal as a whole (`(?:\\.|[^"\\])*`) so that a `)` inside the
  // formatter output cannot terminate the match early.
  const re = /input: "((?:[^"\\]|\\.)*)",\s*result: (Formatted|Failed)\("((?:[^"\\]|\\.)*)"\)/g;
  let mm;
  while ((mm = re.exec(text)) !== null) {
    results.push({ input: mm[1], type: mm[2], value: mm[3] });
  }
  const data = { id, text, settingsText: mSettings ? mSettings[1] : '', configs, results };
  cache.set(id, data);
  return data;
}

async function getInput(name) {
  if (inputsCache.has(name)) return inputsCache.get(name);
  const res = await fetch(`../../test_data/ron_corpus/${name}`);
  const text = await res.text();
  inputsCache.set(name, text);
  return text;
}

async function select(entry) {
  for (const el of list.querySelectorAll('.row')) {
    el.classList.toggle('active', el.dataset.id === entry.id);
  }
  current = await loadMatrix(entry.id);
  detailTitle.textContent = current.id;
  detailMeta.textContent = `${current.configs.length} config source${current.configs.length === 1 ? '' : 's'} · ${current.results.length} inputs`;
  settingsEl.innerHTML = highlightRon(`(${current.settingsText})`);
  sources.replaceChildren();
  for (const c of current.configs) {
    const chip = document.createElement('span');
    chip.className = 'chip';
    chip.textContent = c;
    sources.appendChild(chip);
  }
  renderResults();
}

function renderResults() {
  if (!current) return;
  const rq = (rQ.value || '').toLowerCase().trim();
  const failedOnly = onlyFailed.checked;
  const showOrig = showInput ? showInput.checked : false;
  resultsEl.replaceChildren();
  for (const r of current.results) {
    if (failedOnly && r.type !== 'Failed') continue;
    if (rq && !r.input.toLowerCase().includes(rq) && !r.value.toLowerCase().includes(rq)) continue;
    const item = document.createElement('div');
    item.className = 'result';
    const head = document.createElement('div');
    head.className = 'result-head';
    const name = document.createElement('div');
    name.className = 'result-name';
    name.textContent = r.input;
    const state = document.createElement('div');
    state.className = 'result-state ' + (r.type === 'Failed' ? 'fail' : 'ok');
    state.textContent = r.type === 'Failed' ? 'Failed' : 'Formatted';
    head.appendChild(name);
    head.appendChild(state);
    const body = document.createElement('div');
    body.className = 'result-body';
    const codes = [];
    if (showOrig) {
      const ohead = document.createElement('div');
      ohead.className = 'row-meta';
      ohead.textContent = 'Original';
      const op = document.createElement('pre');
      const ocode = document.createElement('code');
      ocode.className = 'language-ron';
      op.appendChild(ocode);
      body.appendChild(ohead);
      body.appendChild(op);
      codes.push(ocode);
      getInput(r.input).then(t => {
        if (item.classList.contains('open') && ocode.dataset.rendered !== '1') {
          ocode.innerHTML = highlightRon(t);
          ocode.dataset.rendered = '1';
        } else {
          ocode.dataset.orig = t;
        }
      });
    }
    const fhead = document.createElement('div');
    fhead.className = 'row-meta';
    fhead.textContent = r.type === 'Failed' ? 'Error' : 'Formatted';
    const pre = document.createElement('pre');
    const code = document.createElement('code');
    code.className = 'language-ron';
    pre.appendChild(code);
    body.appendChild(fhead);
    body.appendChild(pre);
    codes.push(code);
    // Decode the RON string layer the golden matrix wraps every cell in. The
    // matrix serializes the formatter output as a RON string, so decoding it
    // once recovers exactly what the formatter produced, including literal
    // `\u{...}`, `\x..` and `\n` sequences that belong to the output itself.
    // This has to be a single pass: a chained `replace` would re-read the
    // backslash it just decoded and turn `\\u{1F980}` into the emoji instead
    // of the literal text the formatter emitted.
    const unesc = (s) => s.replace(
      /\\(?:u\{([0-9a-fA-F]+)\}|u([0-9a-fA-F]{4})|x([0-9a-fA-F]{2})|([nrt0"'\\]))/g,
      (m, ubrace, u4, x, simple) => {
        if (ubrace) return String.fromCodePoint(parseInt(ubrace, 16));
        if (u4) return String.fromCodePoint(parseInt(u4, 16));
        if (x) return String.fromCharCode(parseInt(x, 16));
        return { n: '\n', r: '\r', t: '\t', '0': '\0', '"': '"', "'": "'", '\\': '\\' }[simple];
      }
    );
    head.addEventListener('click', () => {
      item.classList.toggle('open');
      if (item.classList.contains('open')) {
        if (showOrig && codes[0]) {
          if (codes[0].dataset.orig && codes[0].dataset.rendered !== '1') {
            codes[0].innerHTML = highlightRon(codes[0].dataset.orig);
            codes[0].dataset.rendered = '1';
          }
        }
        const codeEl = codes[codes.length - 1];
        if (codeEl.dataset.rendered !== '1') {
          codeEl.innerHTML = highlightRon(unesc(r.value));
          codeEl.dataset.rendered = '1';
        }
      }
    });
    item.appendChild(head);
    item.appendChild(body);
    resultsEl.appendChild(item);
  }
}

rQ.addEventListener('input', renderResults);
if (onlyFailed) onlyFailed.addEventListener('change', renderResults);
if (showInput) showInput.addEventListener('change', renderResults);

loadIndex();
