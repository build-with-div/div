// Parse the match in divui's `tag_definition` into rows of the element table.
//
// It fails closed: a tag that produces no row is an error, so an arm in a form
// this parser does not know cannot drop out of the reference unnoticed.

/** `source` without comments, so a tag in a comment is not a table entry. */
function withoutComments(source) {
  let out = '';
  for (let i = 0; i < source.length; ) {
    if (source[i] === '"') {
      let end = i + 1;
      while (end < source.length && source[end] !== '"') end += source[end] === '\\' ? 2 : 1;
      out += source.slice(i, end + 1);
      i = end + 1;
    } else if (source.startsWith('//', i)) {
      const end = source.indexOf('\n', i);
      i = end < 0 ? source.length : end;
    } else if (source.startsWith('/*', i)) {
      const end = source.indexOf('*/', i);
      i = end < 0 ? source.length : end + 2;
    } else {
      out += source[i++];
    }
  }
  return out;
}

// `"a" | "b" => (T::Kind, None | Some(Display::X))`, where rustfmt can wrap
// the patterns over several lines and the value as `=> (\n T::Kind, ... \n)`
// or as a block, `=> {\n (T::Kind, None)\n }`.
const ARM =
  /((?:"[^"\\]*"\s*\|?\s*)+)=>\s*(\{\s*)?\(\s*T::(\w+)\s*,\s*(None|Some\(\s*Display::(\w+)\s*\))\s*,?\s*\)\s*(\})?/g;

/**
 * The rows of a `tag_definition` body, in source order:
 * `{ tag, kind, displayOverride }`. Throws if any quoted tag has no row.
 */
export function parseTagTable(body) {
  const source = withoutComments(body);
  // The catch-all closes the match, so its absence means a truncated body.
  if (!/\b_\s*=>/.test(source)) {
    throw new Error('tag_definition: cannot find the catch-all arm that ends the tag table');
  }
  const rows = [];
  for (const m of source.matchAll(ARM)) {
    const [, patterns, open, kind, , displayOverride, close] = m;
    if (open && !close) continue;
    for (const name of patterns.matchAll(/"([^"\\]*)"/g)) {
      rows.push({ tag: name[1], kind, displayOverride: displayOverride ?? null });
    }
  }

  const unmatched = new Map();
  for (const literal of source.matchAll(/"((?:[^"\\]|\\.)*)"/g)) {
    unmatched.set(literal[1], (unmatched.get(literal[1]) ?? 0) + 1);
  }
  if (unmatched.size === 0) throw new Error('tag_definition: found no tags');
  for (const row of rows) unmatched.set(row.tag, unmatched.get(row.tag) - 1);
  const missing = [...unmatched].filter(([, count]) => count > 0).map(([tag]) => tag);
  if (missing.length > 0) {
    throw new Error(
      `tag_definition: no table row for ${missing.map((tag) => `"${tag}"`).join(', ')}. ` +
        'Teach site/scripts/tag-table.mjs the form of that match arm.',
    );
  }
  return rows;
}
