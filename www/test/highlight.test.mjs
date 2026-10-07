// Node test suite for the RON highlighter (www/highlight.js). The editor
// overlays the highlighted <pre> on a transparent textarea, so the markup
// must contain exactly the input's characters, in order — anything else
// drifts the caret off the glyphs. Run with: node --test www/test/

import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { highlightRon, escapeHtml } from '../highlight.js';

const stripTags = html => html.replace(/<[^>]+>/g, '');

test('highlighting never adds, drops, or reorders characters', () => {
  const dir = new URL('../examples/', import.meta.url);
  for (const file of readdirSync(dir)) {
    const src = readFileSync(new URL(file, dir), 'utf8');
    assert.equal(stripTags(highlightRon(src)), escapeHtml(src), `in ${file}`);
  }
});

test('the invariant also holds for hostile and truncated input', () => {
  const cases = [
    '',
    'unterminated ("string',
    "lone ' quote",
    "'\\'' '\\n' '🦀'",
    '/* unterminated block',
    'r#"unterminated raw',
    'b"byte\nstring" r##"has "# inside"##',
    '#![enable(implicit_some)]',
    '#![unterminated attr',
    '// comment to EOF',
    'a : 1, b:2, C:3',
    '-1.5e-3 0xFFu8 0b1010_1010 1_000_000.isize inf NaN',
    '<&> in a "string" and <bare>',
    '\t\ttabs and \r\n crlf',
    'r#a.b-c: r#fn, café: 名前, 🦀: 1',
    '#![type = "a]b"] 1',
    '1e_3 .5 1. -NaNf32',
  ];
  for (const src of cases) {
    assert.equal(stripTags(highlightRon(src)), escapeHtml(src), `in ${JSON.stringify(src)}`);
  }
});

test('tokens get the expected classes', () => {
  const html = highlightRon('(foo: Some(1.5), bar: "s", c: \'x\') // done');
  assert.match(html, /<span class="tok-key">foo<\/span>/);
  assert.match(html, /<span class="tok-kw">Some<\/span>/);
  assert.match(html, /<span class="tok-num">1\.5<\/span>/);
  assert.match(html, /<span class="tok-str">"s"<\/span>/);
  assert.match(html, /<span class="tok-chr">'x'<\/span>/);
  assert.match(html, /<span class="tok-com">\/\/ done<\/span>/);
  // `bar` is a key even with whitespace before the colon.
  assert.match(html, /<span class="tok-key">bar<\/span>/);
});

test('raw and unicode identifiers, and exponent underscores, are single tokens', () => {
  const html = highlightRon('(r#type: r#a.b-c, café: 名前, e: 1e_3)');
  assert.match(html, /<span class="tok-key">r#type<\/span>/);
  assert.match(html, /<span class="tok-id">r#a\.b-c<\/span>/);
  assert.match(html, /<span class="tok-key">café<\/span>/);
  assert.match(html, /<span class="tok-id">名前<\/span>/);
  assert.match(html, /<span class="tok-num">1e_3<\/span>/);
  // Floats may omit the digits on either side of the point.
  const floats = highlightRon('[.5, 1., 1.e3]');
  for (const f of ['.5', '1.', '1.e3']) assert.ok(floats.includes(`<span class="tok-num">${f}</span>`), f);
  // A `]` inside an attribute's string does not end the attribute.
  assert.match(highlightRon('#![type = "a]b"] 1'), /<span class="tok-attr">#!\[type = "a\]b"\]<\/span>/);
});

test('byte strings are escaped strings, not raw', () => {
  // The escaped quote does not end the byte string; the rest of the line is
  // not swallowed into it.
  const html = highlightRon('(a: b"x\\"y", b: 1)');
  assert.match(html, /<span class="tok-str">b"x\\"y"<\/span>/);
  assert.match(html, /<span class="tok-num">1<\/span>/);
  // A byte char is one token too.
  assert.match(highlightRon("b'a' 2"), /<span class="tok-chr">b'a'<\/span>/);
  // And raw byte strings still end at the matching hashes.
  assert.match(highlightRon('br#"a "" b"# 3'), /<span class="tok-str">br#"a "" b"#<\/span>/);
});

test('nested block comments highlight as one comment', () => {
  const html = highlightRon('/* a /* b */ still comment */ 1');
  assert.match(html, /<span class="tok-com">\/\* a \/\* b \*\/ still comment \*\/<\/span>/);
  assert.match(html, /<span class="tok-num">1<\/span>/);
});

test('raw strings end at the first quote-plus-matching-hashes', () => {
  // "## inside does not close r###"…"###; the first "### does.
  const runs = highlightRon('r###"has "## and "# too"### + 1');
  assert.match(runs, /<span class="tok-str">r###"has "## and "# too"###<\/span>/);
  // …but "## does close r##"…"##.
  const stops = highlightRon('r##"ends"## + 1');
  assert.match(stops, /<span class="tok-str">r##"ends"##<\/span>/);
});

test('html-significant characters are escaped everywhere', () => {
  const html = highlightRon('"<b>&"');
  assert.ok(!html.includes('<b>'));
  assert.equal(stripTags(html), '&quot;&lt;b&gt;&amp;&quot;'.replaceAll('&quot;', '"'));
});
