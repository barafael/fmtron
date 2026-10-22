import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// matrix.js has no exports (it is a page script), so the two pure helpers it
// relies on are mirrored here. Keep them in sync with www/matrix/matrix.js.
const unescapeRon = (s) => s.replace(
  /\\(?:u\{([0-9a-fA-F]+)\}|u([0-9a-fA-F]{4})|x([0-9a-fA-F]{2})|([nrt0"'\\]))/g,
  (m, ubrace, u4, x, simple) => {
    if (ubrace) return String.fromCodePoint(parseInt(ubrace, 16));
    if (u4) return String.fromCodePoint(parseInt(u4, 16));
    if (x) return String.fromCharCode(parseInt(x, 16));
    return { n: '\n', r: '\r', t: '\t', '0': '\0', '"': '"', "'": "'", '\\': '\\' }[simple];
  }
);

const CELL = /input: "([^"]+)",\s*result: (Formatted\(([\s\S]*?)\)|Failed\(([\s\S]*?)\))/g;

function parseMatrix(text) {
  const results = [];
  for (const m of text.matchAll(CELL)) {
    const inner = m[2].startsWith('Formatted') ? m[3] : m[4];
    const quoted = inner.match(/^"([\s\S]*)"$/);
    results.push({
      input: m[1],
      type: m[2].startsWith('Formatted') ? 'Formatted' : 'Failed',
      value: quoted ? quoted[1] : inner.slice(1, -1),
    });
  }
  // `matchAll` does not advance unless the regex is global; a missing `g`
  // made the first version of the page show nothing.
  assert.equal(CELL.global, true);
  return results;
}

function findMatrixFile() {
  const root = new URL('..', import.meta.url);
  return fileURLToPath(new URL('matrix/data/generated-w1-t1-m1-d16-keep.ron', root));
}

test('the matrix parser finds every cell', () => {
  const text = readFileSync(findMatrixFile(), 'utf8');
  const cells = parseMatrix(text);
  const inputs = [...text.matchAll(/input: "([^"]+)"/g)].length;
  assert.equal(cells.length, inputs);
  assert.ok(cells.length > 100, `only ${cells.length} cells`);
});

test('a formatted cell is unescaped to the formatter output', () => {
  const text = readFileSync(findMatrixFile(), 'utf8');
  const cell = parseMatrix(text).find((c) => c.input === 'edge_blank_lines_list.ron');
  assert.equal(unescapeRon(cell.value), '[\n 1,\n\n 2,\n\n // between elements\n\n 3,\n]');
});

test('literal formatter output survives decoding', () => {
  // The golden matrix wraps the formatter output in a RON string. Literal
  // `\u{...}` / `\x..` text the formatter emitted must stay literal; only the
  // RON layer itself is decoded.
  assert.equal(unescapeRon('\\"\\\\u{1F980}\\"'), '"\\u{1F980}"');
  assert.equal(unescapeRon("\\'\\\\u{1F980}\\'"), "'\\u{1F980}'");
  // The JS source string is  b\\"{\\\\x00\\\\x00}\\"  (RON for a byte string).
  assert.equal(unescapeRon('b\\"{\\\\x00\\\\x00}\\"'), 'b"{\\x00\\x00}"');
  // A literal backslash-n in the output is not a newline.
  assert.equal(unescapeRon('\\"a\\\\nb\\"'), '"a\\nb"');
  // Real RON escapes are decoded exactly once.
  assert.equal(unescapeRon('a\\nb'), 'a\nb');
  assert.equal(unescapeRon('\\"\\\\\\"\\"'), '"\\""');
});

test('matrix settings and config sources parse out of a row', () => {
  const text = readFileSync(findMatrixFile(), 'utf8');
  const settings = text.match(/\n    settings: \(([\s\S]*?)\n    \),/);
  const configs = text.match(/\n    configs: \[([\s\S]*?)\n    \],/);
  assert.ok(settings && configs);
  assert.match(settings[1], /max_width: 1/);
  const names = [...configs[1].matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1]);
  assert.deepEqual(names, ['generated-w1-t1-m1-d16-keep.ron']);
});

test('the committed catalog index matches the matrix rows', () => {
  const root = new URL('..', import.meta.url);
  const index = JSON.parse(readFileSync(fileURLToPath(new URL('matrix/data/index.json', root)), 'utf8'));
  assert.ok(index.count > 300, `only ${index.count} rows`);
  const row = index.entries.find((entry) => entry.id === 'generated-w1-t1-m1-d16-keep');
  assert.ok(row, 'row is missing from the index');
  assert.ok(row.settings.includes('max_width: 1'));
  assert.ok(row.inputs_count > 100);
  assert.deepEqual(row.configs, ['generated-w1-t1-m1-d16-keep.ron']);
});
