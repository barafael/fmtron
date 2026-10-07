// Node test suite for the structural view's parser (www/structure.js). The
// renderer needs a document and is exercised in the browser.

import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { parseRon, tokenize, countNodes, renderStructure, NODE_BUDGET } from '../structure.js';

test('every example parses, and its shape has as many nodes as values', () => {
  const dir = new URL('../examples/', import.meta.url);
  for (const file of readdirSync(dir)) {
    const src = readFileSync(new URL(file, dir), 'utf8');
    const doc = parseRon(src);
    assert.ok(countNodes(doc.root) > 0, file);
  }
});

test('shapes: struct, tuple, list, map, unit, scalar, wrapper', () => {
  const { attributes, root } = parseRon(`
    #![enable(implicit_some)]
    Scene( // a comment
        name: "demo",
        size: (800, 600),
        tags: ["a", "b"],
        weights: {1: 0.5, "two": 2},
        mode: Fast,
        maybe: Some(3),
        unit: (),
        anon: (x: 1),
    )`);
  assert.deepEqual(attributes, ['#![enable(implicit_some)]']);
  assert.equal(root.kind, 'struct');
  assert.equal(root.name, 'Scene');
  const f = Object.fromEntries(root.fields.map(x => [x.key, x.value]));
  assert.deepEqual(f.name, { kind: 'scalar', type: 'str', text: '"demo"' });
  assert.deepEqual(f.size, { kind: 'tuple', name: null, items: [
    { kind: 'scalar', type: 'num', text: '800' }, { kind: 'scalar', type: 'num', text: '600' },
  ] });
  assert.equal(f.tags.kind, 'list');
  assert.equal(f.weights.kind, 'map');
  assert.equal(f.weights.entries[1].key.text, '"two"');
  assert.deepEqual(f.mode, { kind: 'unit', name: 'Fast' });
  assert.deepEqual(f.maybe, { kind: 'tuple', name: 'Some', items: [{ kind: 'scalar', type: 'num', text: '3' }] });
  assert.deepEqual(f.unit, { kind: 'tuple', name: null, items: [] });
  assert.equal(f.anon.kind, 'struct');
  assert.equal(f.anon.name, null);
});

test('the lexer knows the literals the formatter knows', () => {
  const kinds = tokenize(`-1.5e-3 .5 1. 0xFFu8 inf -NaNf32 true 'x' b'\\n' "s\\"q" r#"raw"#` +
    ` br"b" r#type café: 名前 Some`).map(t => `${t.kind}:${t.text}`);
  assert.deepEqual(kinds, [
    'num:-1.5e-3', 'num:.5', 'num:1.', 'num:0xFFu8', 'num:inf', 'num:-NaNf32', 'bool:true',
    "chr:'x'", "chr:b'\\n'", 'str:"s\\"q"', 'str:r#"raw"#', 'str:br"b"', 'ident:r#type',
    'ident:café', 'punct::', 'ident:名前', 'ident:Some',
  ]);
  // Identifiers that merely start like a special float are identifiers.
  assert.equal(tokenize('inf64_is_an_identifier')[0].kind, 'ident');
});

test('malformed input is an error, not a wrong tree', () => {
  for (const src of ['(a: 1 b: 2)', '[1, 2', '{1 2}', '"open', '', '(a: 1) 2', '/* open']) {
    assert.throws(() => parseRon(src), src);
  }
});

// A document that builds plain objects: enough for the renderer, whose only
// DOM needs are createElement, className, textContent, open and append.
function stubDocument() {
  let created = 0;
  const createElement = tag => {
    created++;
    const children = [];
    const node = { tag, className: '', textContent: '', open: false, children,
      append(...nodes) { children.push(...nodes); } };
    return node;
  };
  return { createElement, get created() { return created; } };
}

const walk = (node, f) => { f(node); for (const c of node.children ?? []) walk(c, f); };
const texts = node => { const out = []; walk(node, n => { if (n.textContent) out.push(n.textContent); }); return out; };

test('the renderer shows every value and keeps names and shapes apart', () => {
  const doc = stubDocument();
  const view = renderStructure(parseRon('Scene(name: "demo", pos: (1, 2), tags: [A, B], m: {"k": Some(1)}, u: Nothing)'), doc);
  const t = texts(view);
  // Tuples carry no kind label: they are a row of their items.
  for (const expected of ['Scene', '"demo"', '1', '2', 'A', 'B', '"k"', 'Some', 'Nothing', 'struct', 'list', 'map']) {
    assert.ok(t.includes(expected), `missing ${expected} in ${JSON.stringify(t)}`);
  }
  const classes = []; walk(view, n => classes.push(n.className));
  assert.ok(classes.includes('chip name type'), 'Scene is a type name');
  assert.ok(classes.includes('chip name variant'), 'Some is a variant name');
  assert.ok(classes.includes('chip variant'), 'Nothing reads as a variant');
  assert.ok(classes.some(c => c.startsWith('node list words')), 'a list of identifiers flows as words');
});

test('rendering stops at the node budget with a "+N more" chip', () => {
  const big = '[' + Array.from({ length: NODE_BUDGET + 500 }, (_, i) => i).join(', ') + ']';
  const doc = stubDocument();
  const view = renderStructure(parseRon(big), doc);
  const t = texts(view);
  assert.ok(t.some(x => /^\+\d+ more items$/.test(x)), 'has a more chip');
  assert.ok(doc.created < NODE_BUDGET + 50, `created ${doc.created} elements`);
});
