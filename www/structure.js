// The structural view of a RON document: a small parser that turns the text
// into a tree of shapes, and a renderer that lays that tree out as nested
// boxes. Pure functions over strings and a `document`, so the parser is also
// exercised by the Node test suite (www/test/).
//
// Two things are shown on separate axes. The *shape* of a value (struct,
// tuple, list, map, scalar) decides its box and background; the *name* on a
// struct or tuple, or a bare identifier, is a chip whose colour says whether
// it reads as a type (`Point(…)`) or as an enum variant (`Red`, `Some(…)`).
// RON itself cannot tell a struct from a struct-like variant, so the latter
// is a reading, not a fact: capitalised names used without a body, and the
// standard library's Option and Result variants, count as variants.

const VARIANT_NAMES = new Set(['Some', 'None', 'Ok', 'Err']);

// --- Tokens ------------------------------------------------------------------

const NUMBER = /[-+]?(?:0[xob][0-9a-fA-F_]+|(?:[0-9][0-9_]*(?:\.[0-9_]*)?|\.[0-9_]+)(?:[eE][-+]?[0-9_]+)?)(?:[iuf](?:8|16|32|64|128|size))?/y;
const IDENT = /r#[\p{ID_Continue}.+-]+|[\p{ID_Start}_]\p{ID_Continue}*/uy;
const RAW_STRING = /b?r(#*)"/y;
const SPECIAL = /[-+]?(?:inf|NaN)(?:f32|f64)?(?![\p{ID_Continue}])/uy;

class ParseError extends Error {}

// Lexes `src` into [{kind, text}] with comments and whitespace dropped.
// `kind` is one of: num, str, chr, bool, ident, punct, attr.
export function tokenize(src) {
  const toks = [];
  const n = src.length;
  let i = 0;
  while (i < n) {
    const c = src[i];
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r') { i++; continue; }
    if (c === '/' && src[i + 1] === '/') {
      const j = src.indexOf('\n', i);
      i = j === -1 ? n : j;
      continue;
    }
    if (c === '/' && src[i + 1] === '*') {
      let depth = 0;
      let j = i;
      while (j < n) {
        if (src[j] === '/' && src[j + 1] === '*') { depth++; j += 2; }
        else if (src[j] === '*' && src[j + 1] === '/') { depth--; j += 2; if (depth === 0) break; }
        else j++;
      }
      i = j;
      continue;
    }
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
      toks.push({ kind: 'attr', text: src.slice(i, stop) });
      i = stop;
      continue;
    }
    RAW_STRING.lastIndex = i;
    const raw = RAW_STRING.exec(src);
    if (raw) {
      const closer = '"' + '#'.repeat(raw[1].length);
      const end = src.indexOf(closer, i + raw[0].length);
      if (end === -1) throw new ParseError('unterminated raw string');
      toks.push({ kind: 'str', text: src.slice(i, end + closer.length) });
      i = end + closer.length;
      continue;
    }
    const byte = c === 'b' ? 1 : 0;
    const q = src[i + byte];
    if (q === '"' || q === "'") {
      let j = i + byte + 1;
      while (j < n && src[j] !== q) j += src[j] === '\\' ? 2 : 1;
      if (j >= n) throw new ParseError('unterminated literal');
      toks.push({ kind: q === '"' ? 'str' : 'chr', text: src.slice(i, j + 1) });
      i = j + 1;
      continue;
    }
    SPECIAL.lastIndex = i;
    const special = SPECIAL.exec(src);
    if (special) {
      toks.push({ kind: 'num', text: special[0] });
      i = SPECIAL.lastIndex;
      continue;
    }
    if (c === '-' || c === '+' || c === '.' || (c >= '0' && c <= '9')) {
      NUMBER.lastIndex = i;
      const num = NUMBER.exec(src);
      if (num && num[0] !== '') {
        toks.push({ kind: 'num', text: num[0] });
        i = NUMBER.lastIndex;
        continue;
      }
    }
    IDENT.lastIndex = i;
    const ident = IDENT.exec(src);
    if (ident) {
      const word = ident[0];
      toks.push({ kind: word === 'true' || word === 'false' ? 'bool' : 'ident', text: word });
      i = IDENT.lastIndex;
      continue;
    }
    if ('()[]{}:,'.includes(c)) {
      toks.push({ kind: 'punct', text: c });
      i++;
      continue;
    }
    throw new ParseError(`unexpected character ${JSON.stringify(c)}`);
  }
  return toks;
}

// --- Parser ------------------------------------------------------------------
//
// Nodes:
//   { kind: 'struct', name, fields: [{ key, value }] }   Name(a: 1) or (a: 1)
//   { kind: 'tuple',  name, items: [value] }             Name(1, 2) or (1, 2); () is an empty tuple
//   { kind: 'list',   items: [value] }
//   { kind: 'map',    entries: [{ key, value }] }
//   { kind: 'unit',   name }                             a bare identifier
//   { kind: 'scalar', type: 'num' | 'str' | 'chr' | 'bool', text }
// `name` is null when absent.

export function parseRon(src) {
  const toks = tokenize(src);
  let pos = 0;
  const peek = (k = 0) => toks[pos + k];
  const next = () => {
    if (pos >= toks.length) throw new ParseError('unexpected end of input');
    return toks[pos++];
  };
  const expect = text => {
    const t = next();
    if (t.kind !== 'punct' || t.text !== text) throw new ParseError(`expected ${text}, found ${t.text}`);
  };
  const isPunct = (t, text) => t && t.kind === 'punct' && t.text === text;

  const attributes = [];
  while (peek() && peek().kind === 'attr') attributes.push(next().text);

  function value() {
    const t = next();
    switch (t.kind) {
      case 'num': case 'str': case 'chr': case 'bool':
        return { kind: 'scalar', type: t.kind, text: t.text };
      case 'ident':
        if (isPunct(peek(), '(')) return parens(t.text);
        return { kind: 'unit', name: t.text };
      case 'punct':
        if (t.text === '(') { pos--; return parens(null); }
        if (t.text === '[') return list();
        if (t.text === '{') return map();
        break;
      default:
    }
    throw new ParseError(`unexpected ${t.text}`);
  }

  // After an optional name: `(` then fields or items.
  function parens(name) {
    expect('(');
    if (isPunct(peek(), ')')) { next(); return { kind: 'tuple', name, items: [] }; }
    const named = peek().kind === 'ident' && isPunct(peek(1), ':');
    if (named) {
      const fields = [];
      while (!isPunct(peek(), ')')) {
        const key = next();
        if (key.kind !== 'ident') throw new ParseError(`expected a field name, found ${key.text}`);
        expect(':');
        fields.push({ key: key.text, value: value() });
        if (isPunct(peek(), ',')) next(); else break;
      }
      expect(')');
      return { kind: 'struct', name, fields };
    }
    const items = [];
    while (!isPunct(peek(), ')')) {
      items.push(value());
      if (isPunct(peek(), ',')) next(); else break;
    }
    expect(')');
    return { kind: 'tuple', name, items };
  }

  function list() {
    const items = [];
    while (!isPunct(peek(), ']')) {
      items.push(value());
      if (isPunct(peek(), ',')) next(); else break;
    }
    expect(']');
    return { kind: 'list', items };
  }

  function map() {
    const entries = [];
    while (!isPunct(peek(), '}')) {
      const key = value();
      expect(':');
      entries.push({ key, value: value() });
      if (isPunct(peek(), ',')) next(); else break;
    }
    expect('}');
    return { kind: 'map', entries };
  }

  if (pos >= toks.length) throw new ParseError('no value');
  const root = value();
  if (pos < toks.length) throw new ParseError(`expected end of input, found ${toks[pos].text}`);
  return { attributes, root };
}

// --- Rendering ---------------------------------------------------------------

// Rendering stops adding children past this many nodes; the rest of each
// container is summed up as "+N more", so a huge file stays a page, not a
// million elements.
export const NODE_BUDGET = 4000;

const isContainer = node => node.kind === 'struct' || node.kind === 'tuple' || node.kind === 'list' || node.kind === 'map';
const isVariantName = name => VARIANT_NAMES.has(name) || /^\p{Lu}/u.test(name);
const scalarWord = node => node.kind === 'scalar' || node.kind === 'unit';
// A wrapper such as `Some(x)` or `Px(12)`: one unnamed item behind a name.
const isWrapper = node => node.kind === 'tuple' && node.name !== null && node.items.length === 1;

// Builds the structural view of `doc` (from parseRon) as a DOM element.
export function renderStructure(doc, document) {
  let budget = NODE_BUDGET;
  const el = (tag, cls, text) => {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  };
  const chip = (cls, text) => el('span', `chip ${cls}`, text);
  const plural = (n, word) => `${n} ${word}${n === 1 ? '' : 's'}`;

  // The name on a body (`Point(…)`) reads as a type, unless it is one of
  // the standard variants; a bare capitalised identifier reads as a variant.
  function nameChip(name) {
    return chip(VARIANT_NAMES.has(name) ? 'name variant' : 'name type', name);
  }

  function more(n) {
    return chip('more', `+${plural(n, 'more item')}`);
  }

  // Renders the children of a container through `renderChild`, stopping at
  // the budget.
  function children(items, container, renderChild) {
    for (let i = 0; i < items.length; i++) {
      if (budget <= 0) { container.append(more(items.length - i)); return; }
      budget--;
      container.append(renderChild(items[i], i));
    }
  }

  function render(node) {
    switch (node.kind) {
      case 'scalar':
        return chip(node.type, node.text);
      case 'unit':
        return chip(isVariantName(node.name) ? 'variant' : 'type', node.name);
      case 'struct': {
        const box = el('details', 'node struct');
        box.open = true;
        const head = el('summary');
        if (node.name !== null) head.append(nameChip(node.name));
        head.append(el('span', 'kind', 'struct'), el('span', 'count', plural(node.fields.length, 'field')));
        box.append(head);
        const grid = el('div', 'fields');
        children(node.fields, grid, f => {
          const row = el('div', 'row');
          row.append(el('span', 'key', f.key), slot(f.value));
          return row;
        });
        box.append(grid);
        return box;
      }
      case 'tuple': {
        if (isWrapper(node)) {
          const wrap = el('span', 'wrapper');
          wrap.append(nameChip(node.name), render(node.items[0]));
          return wrap;
        }
        const box = el('div', 'node tuple');
        if (node.name !== null) box.append(nameChip(node.name));
        const row = el('div', 'items');
        if (node.items.length === 0) row.append(chip('empty', '()'));
        children(node.items, row, render);
        box.append(row);
        return box;
      }
      case 'list': {
        const words = node.items.every(scalarWord);
        const box = el('details', `node list ${words ? 'words' : 'cards'}`);
        box.open = true;
        const head = el('summary');
        head.append(el('span', 'kind', 'list'), el('span', 'count', plural(node.items.length, 'item')));
        box.append(head);
        const flow = el('div', 'items');
        if (node.items.length === 0) flow.append(chip('empty', '[]'));
        children(node.items, flow, render);
        box.append(flow);
        return box;
      }
      case 'map': {
        const box = el('details', 'node map');
        box.open = true;
        const head = el('summary');
        head.append(el('span', 'kind', 'map'), el('span', 'count', plural(node.entries.length, 'entry').replace('entrys', 'entries')));
        box.append(head);
        const grid = el('div', 'entries');
        if (node.entries.length === 0) grid.append(chip('empty', '{}'));
        children(node.entries, grid, e => {
          const row = el('div', 'row');
          row.append(slot(e.key, 'mapkey'), slot(e.value));
          return row;
        });
        box.append(grid);
        return box;
      }
      default:
        return chip('', '?');
    }
  }

  // A value in a row: inline when it is a word, a nested box otherwise.
  function slot(node, cls = 'val') {
    const s = el('div', `${cls}${isContainer(node) && !isWrapper(node) ? ' nested' : ''}`);
    s.append(render(node));
    return s;
  }

  const view = el('div', 'structure-root');
  if (doc.attributes.length > 0) {
    const strip = el('div', 'attributes');
    for (const a of doc.attributes) strip.append(chip('attr', a));
    view.append(strip);
  }
  view.append(render(doc.root));
  return view;
}

// Counts the nodes of a tree; for tests and for deciding whether a document
// is worth rendering.
export function countNodes(node) {
  switch (node.kind) {
    case 'struct': return 1 + node.fields.reduce((n, f) => n + countNodes(f.value), 0);
    case 'tuple': case 'list': return 1 + node.items.reduce((n, v) => n + countNodes(v), 0);
    case 'map': return 1 + node.entries.reduce((n, e) => n + countNodes(e.key) + countNodes(e.value), 0);
    default: return 1;
  }
}
