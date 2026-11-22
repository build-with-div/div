import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseTagTable } from './tag-table.mjs';

const table = (arms) => `fn tag_definition(tag: &str) {
    let definition = match normalized.as_str() {
${arms}
        _ => return Err(Error::UnknownTag(tag.to_owned())),
    };
}`;

const tags = (rows) => rows.map((row) => row.tag);

test('reads one-line arms and patterns wrapped over several lines', () => {
  const rows = parseTagTable(
    table(`"div" | "main"
        | "nav" => (T::Div, None),
        "li" => (T::Div, Some(Display::ListItem)),`),
  );
  assert.deepEqual(rows, [
    { tag: 'div', kind: 'Div', displayOverride: null },
    { tag: 'main', kind: 'Div', displayOverride: null },
    { tag: 'nav', kind: 'Div', displayOverride: null },
    { tag: 'li', kind: 'Div', displayOverride: 'ListItem' },
  ]);
});

test('reads an arm rustfmt wrapped in a block', () => {
  const rows = parseTagTable(
    table(`"span" | "a"
        | "b" => {
            (T::Span, None)
        }
        "li" => {
            (T::Div, Some(Display::ListItem))
        }
        "br" => (T::Break, None),`),
  );
  assert.deepEqual(tags(rows), ['span', 'a', 'b', 'li', 'br']);
  assert.equal(rows[2].kind, 'Span');
  assert.equal(rows[3].displayOverride, 'ListItem');
});

test('reads a value rustfmt wrapped over several lines', () => {
  const rows = parseTagTable(
    table(`"colgroup" => (
            T::TableColumnGroup,
            Some(Display::TableColumnGroup),
        ),`),
  );
  assert.deepEqual(rows, [
    { tag: 'colgroup', kind: 'TableColumnGroup', displayOverride: 'TableColumnGroup' },
  ]);
});

test('ignores tags and braces in comments', () => {
  const rows = parseTagTable(
    table(`// "old" was here }
        /* "older" { */
        "div" => (T::Div, None),`),
  );
  assert.deepEqual(tags(rows), ['div']);
});

test('leaves out tags the table rejects, wrapped or not', () => {
  const rows = parseTagTable(
    table(`"div" => (T::Div, None),
        "html" | "body" => return Err(Error::UnknownTag(name.to_owned())),
        "area" | "base"
        | "template" => {
            return Err(Error::UnknownTag(name.to_owned()))
        }
        _ if is_valid_element_name(name) => (T::Span, None),`),
  );
  assert.deepEqual(tags(rows), ['div']);
});

test('fails on a rejecting arm in a form it cannot read', () => {
  for (const arm of [
    '"mark" => return Err(Error::Other),',
    '"mark" => { log(); return Err(Error::UnknownTag(name.to_owned())) }',
  ]) {
    assert.throws(
      () => parseTagTable(table(`"div" => (T::Div, None),\n${arm}`)),
      /no table row for "mark"/,
      arm,
    );
  }
});

test('fails when a quoted tag produces no row', () => {
  for (const arm of [
    '"mark" => make_mark(),',
    '"mark" => { let kind = T::Span; (kind, None) }',
    '"mark" if enabled => (T::Span, None),',
    '"mark" => (T::Span, Some(other())),',
    '"mark" => { (T::Span, None); other() }',
  ]) {
    assert.throws(
      () => parseTagTable(table(`"div" => (T::Div, None),\n${arm}`)),
      /no table row for "mark"/,
      arm,
    );
  }
});

test('names every tag of an arm it cannot read', () => {
  assert.throws(
    () => parseTagTable(table(`"b" | "i" => make_inline(),`)),
    /no table row for "b", "i"/,
  );
});

test('reads a tag with a hyphen', () => {
  const rows = parseTagTable(table(`"font-face" => (T::Div, None),`));
  assert.deepEqual(tags(rows), ['font-face']);
});

test('fails on a truncated or empty table', () => {
  assert.throws(() => parseTagTable('"div" => (T::Div, None),'), /catch-all/);
  assert.throws(() => parseTagTable(table('')), /found no tags/);
});
