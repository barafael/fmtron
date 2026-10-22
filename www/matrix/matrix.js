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
const resultsEl = document.getElementById('results');

let index = null;
let current = null;
let cache = new Map();

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
  // naive parse of our RON shape
  const mSettings = text.match(/settings: \(([^()]*(\([^()]*\)[^()]*)*)\)/s) || text.match(/settings: \(([\s\S]*?)\)\s*,\s*results:/);
  const configs = [...text.matchAll(/configs: \[\s*([\s\S]*?)\s*\]/s)].flatMap(m => [...m[1].matchAll(/"([^"]+)"/g)].map(x => x[1]));
  const results = [];
  const re = /input: "([^"]+)",\s*result: (Formatted\(([\s\S]*?)\)|Failed\(([\s\S]*?)\))/g;
  let mm;
  while ((mm = re.exec(text)) !== null) {
    if (mm[2].startsWith('Formatted')) {
      // extract string content inside outer parens and quotes
      const inner = mm[3];
      const mstr = inner.match(/^"([\s\S]*)"$/);
      results.push({ input: mm[1], type: 'Formatted', value: mstr ? mstr[1] : inner.slice(1, -1) });
    } else {
      const inner = mm[4];
      const mstr = inner.match(/^"([\s\S]*)"$/);
      results.push({ input: mm[1], type: 'Failed', value: mstr ? mstr[1] : inner.slice(1, -1) });
    }
  }
  const data = { id, text, settingsText: mSettings ? mSettings[1] : '', configs, results };
  cache.set(id, data);
  return data;
}

function unescapeRonString(s) {
  // very rough for display; highlight will still work on raw
  return s;
}

async function select(entry) {
  for (const el of list.querySelectorAll('.row')) {
    el.classList.toggle('active', el.dataset.id === entry.id);
  }
  current = await loadMatrix(entry.id);
  detailTitle.textContent = current.id;
  detailMeta.textContent = `${current.configs.length} config source${current.configs.length === 1 ? '' : 's'} · ${current.results.length} inputs`;
  settingsEl.textContent = `(${current.settingsText})`;
  if (window.hljs) {
    delete settingsEl.dataset.highlighted;
    window.hljs.highlightElement(settingsEl);
  }
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
    const pre = document.createElement('pre');
    const code = document.createElement('code');
    code.className = 'language-ron';
    // unescape basic ron string escapes
    const unesc = (s) => s.replace(/\\n/g, '\n').replace(/\\t/g, '\t').replace(/\\"/g, '"').replace(/\\\\/g, '\\');
    code.textContent = unesc(r.value);
    pre.appendChild(code);
    body.appendChild(pre);
    head.addEventListener('click', () => {
      item.classList.toggle('open');
      if (item.classList.contains('open') && window.hljs && !code.dataset.highlighted) {
        window.hljs.highlightElement(code);
      }
    });
    item.appendChild(head);
    item.appendChild(body);
    resultsEl.appendChild(item);
  }
}

rQ.addEventListener('input', renderResults);
onlyFailed.addEventListener('change', renderResults);

loadIndex();
