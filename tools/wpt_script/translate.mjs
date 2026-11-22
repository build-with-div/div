#!/usr/bin/env node
// RFC 0005 front end: find the testharness subtests a WPT page declares and
// the web surfaces each one uses.
//
// This is a new module beside tools/wpt/javascript_mutation_ir.mjs, whose
// output the renderer contract pins; it shares no code with it.
//
// Input (stdin, JSON): [{file, title, scripts: [{source, offset}], helpers: [names]}]
//   `helpers` are the intrinsic helper libraries the page loads; their
//   subtest-declaring functions (for example ResizeTestHelper) are known by
//   name, never interpreted.
// Output (stdout, JSON): one record per file:
//   {file, parse_error?, subtests: [{name, name_known, declared_by, surfaces,
//    untranslatable?}], page_surfaces, chains: [[{label, surfaces, reasons,
//    subtests, finishes}]]}
//
// A subtest's surfaces are every web surface its declaring call reaches
// statically: the call's own arguments, the named top-level function that
// encloses it, the top-level functions those call (transitively) and the
// page's top-level statements. Nothing is dropped: a construct the runtime
// cannot express marks the subtest untranslatable with a reason.
//
// `chains` are the page's top-level promise chains, `f().then(() => g())...`,
// link by link. A link's surfaces are what it runs outside a synchronous
// `test()`, which catches whatever its body throws; if one of them cannot
// run, no later link does (ledger.py's reachability).

import {pathToFileURL} from 'node:url';

const acornPath = process.argv[2];
if (!acornPath) throw new Error('usage: translate.mjs ACORN_MJS [--ast] < pages.json');
const {parse} = await import(pathToFileURL(acornPath).href);

let input = '';
for await (const chunk of process.stdin) input += chunk;
const pages = JSON.parse(input);

const HARNESS_DECLARATIONS = new Set(['test', 'async_test', 'promise_test']);
// Intrinsic helper constructors and functions that declare a subtest, with
// the argument index of the subtest name.
const HELPER_DECLARATIONS = new Map([
  ['ResizeTestHelper', 0],
]);
// Names testharness.js and the intrinsic helpers provide. They are runtime
// surfaces (`harness:`), not web surfaces.
const HARNESS_NAMES = new Set([
  'test', 'async_test', 'promise_test', 'setup', 'promise_setup', 'done',
  'add_completion_callback', 'add_result_callback', 'step_timeout',
  'format_value', 'EventWatcher', 'promise_rejects_js', 'promise_rejects_dom',
  'promise_rejects_exactly', 'fetch_tests_from_window',
]);
const GLOBAL_OBJECTS = new Set(['window', 'document', 'self', 'globalThis', 'navigator',
  'performance', 'console', 'Math', 'JSON', 'Object', 'Array', 'Number', 'String',
  'Promise', 'Symbol', 'Reflect', 'Date', 'Error', 'TypeError', 'RangeError', 'Map', 'Set',
  'test_driver', 'internals', 'eventSender', 'testRunner', 'CSS', 'location', 'screen']);
// JavaScript itself, available to every generated test without a Div call.
const LANGUAGE = new Set(['Math', 'JSON', 'Object', 'Array', 'Number', 'String', 'Promise',
  'Symbol', 'Reflect', 'Map', 'Set', 'Error', 'TypeError', 'RangeError', 'isNaN', 'isFinite',
  'parseInt', 'parseFloat', 'undefined', 'NaN', 'Infinity', 'Boolean', 'console']);
// Constructs a straight-line Rust test cannot express.
const UNTRANSLATABLE = new Map([
  ['eval', 'dynamic-code'],
  ['Function', 'dynamic-code'],
  ['WithStatement', 'with-statement'],
  ['document.write', 'document-write'],
]);

function propertyName(node) {
  if (!node) return null;
  if (!node.computed && node.property?.type === 'Identifier') return node.property.name;
  if (node.computed && node.property?.type === 'Literal') return String(node.property.value);
  return null;
}

function constantString(node) {
  if (!node) return null;
  if (node.type === 'Literal' && typeof node.value === 'string') return node.value;
  if (node.type === 'TemplateLiteral' && node.expressions.length === 0) {
    return node.quasis.map(part => part.value.cooked).join('');
  }
  if (node.type === 'BinaryExpression' && node.operator === '+') {
    const left = constantString(node.left);
    const right = constantString(node.right);
    if (left !== null && right !== null) return left + right;
  }
  return null;
}

function rootName(node) {
  while (node?.type === 'MemberExpression') node = node.object;
  if (node?.type === 'Identifier') return node.name;
  if (node?.type === 'ThisExpression') return 'this';
  return null;
}

function* children(node) {
  for (const key of Object.keys(node)) {
    if (key === 'type' || key === 'start' || key === 'end') continue;
    const value = node[key];
    if (Array.isArray(value)) {
      for (const item of value) if (item && typeof item.type === 'string') yield [item, key];
    } else if (value && typeof value.type === 'string') {
      yield [value, key];
    }
  }
}

// Bindings declared anywhere on the page: locals are not web surfaces.
function declaredNames(program) {
  const names = new Set();
  const visit = node => {
    if (node.type === 'VariableDeclarator' && node.id.type === 'Identifier') names.add(node.id.name);
    if ((node.type === 'FunctionDeclaration' || node.type === 'FunctionExpression') && node.id) {
      names.add(node.id.name);
    }
    if (node.type === 'ClassDeclaration' && node.id) names.add(node.id.name);
    if (['FunctionDeclaration', 'FunctionExpression', 'ArrowFunctionExpression'].includes(node.type)) {
      for (const param of node.params) collectPattern(param, names);
    }
    if (node.type === 'CatchClause' && node.param) collectPattern(node.param, names);
    if (node.type === 'VariableDeclarator' && node.id.type !== 'Identifier') collectPattern(node.id, names);
    for (const [child] of children(node)) visit(child);
  };
  visit(program);
  return names;
}

function collectPattern(pattern, names) {
  if (!pattern) return;
  if (pattern.type === 'Identifier') names.add(pattern.name);
  else if (pattern.type === 'AssignmentPattern') collectPattern(pattern.left, names);
  else if (pattern.type === 'RestElement') collectPattern(pattern.argument, names);
  else if (pattern.type === 'ArrayPattern') pattern.elements.forEach(e => collectPattern(e, names));
  else if (pattern.type === 'ObjectPattern') {
    for (const property of pattern.properties) {
      collectPattern(property.type === 'RestElement' ? property.argument : property.value, names);
    }
  }
}

// The web surfaces a subtree uses, and the top-level functions it calls.
function surfacesOf(node, locals, helpers, skip = () => false) {
  const surfaces = new Set();
  const calls = new Set();
  const reasons = new Set();
  const visit = (node, parent, key) => {
    if (parent && skip(node)) return;
    if (UNTRANSLATABLE.has(node.type)) reasons.add(UNTRANSLATABLE.get(node.type));
    if (node.type === 'CallExpression' || node.type === 'NewExpression') {
      const callee = node.callee;
      const prefix = node.type === 'NewExpression' ? 'new ' : '';
      if (callee.type === 'Identifier') {
        const name = callee.name;
        if (UNTRANSLATABLE.has(name)) reasons.add(UNTRANSLATABLE.get(name));
        if (locals.has(name)) calls.add(name);
        else if (HARNESS_NAMES.has(name) || name.startsWith('assert_') || helpers.has(name)) {
          surfaces.add(`harness:${name}`);
        } else if (!LANGUAGE.has(name)) surfaces.add(`${prefix}${name}()`);
      } else if (callee.type === 'MemberExpression') {
        const method = propertyName(callee);
        const root = rootName(callee.object);
        const qualified = `${root}.${method}`;
        if (UNTRANSLATABLE.has(qualified)) reasons.add(UNTRANSLATABLE.get(qualified));
        if (root && GLOBAL_OBJECTS.has(root) && !locals.has(root)) {
          if (!LANGUAGE.has(root)) surfaces.add(`${prefix}${pathOf(callee)}()`);
        } else if (method && /^(step|step_func|step_func_done|unreached_func|done|add_cleanup|step_timeout|step_wait|step_wait_func|step_wait_func_done|wait_for)$/.test(method)) {
          surfaces.add(`harness:t.${method}`);
        } else if (method) {
          surfaces.add(`${prefix}.${method}()`);
        }
      }
    } else if (node.type === 'MemberExpression' && !(parent?.type === 'CallExpression' && key === 'callee')
               && !(parent?.type === 'NewExpression' && key === 'callee')
               && !(parent?.type === 'MemberExpression' && key === 'object')) {
      const root = rootName(node);
      const write = parent?.type === 'AssignmentExpression' && key === 'left'
        || parent?.type === 'UpdateExpression';
      const path = memberSurface(node, root, locals);
      if (path) surfaces.add(write ? `${path}=` : path);
    } else if (node.type === 'Identifier' && parent && isReference(parent, key)
               && !locals.has(node.name) && !LANGUAGE.has(node.name) && !HARNESS_NAMES.has(node.name)
               && !helpers.has(node.name) && !GLOBAL_OBJECTS.has(node.name)
               && !node.name.startsWith('assert_')) {
      surfaces.add(node.name);
    }
    for (const [child, childKey] of children(node)) visit(child, node, childKey);
  };
  visit(node, null, null);
  return {surfaces, calls, reasons};
}

function isReference(parent, key) {
  if (parent.type === 'MemberExpression' && key === 'property' && !parent.computed) return false;
  if (parent.type === 'Property' && key === 'key' && !parent.computed) return false;
  if (parent.type === 'MethodDefinition' && key === 'key') return false;
  if (['VariableDeclarator'].includes(parent.type) && key === 'id') return false;
  if (['FunctionDeclaration', 'FunctionExpression', 'ArrowFunctionExpression'].includes(parent.type)
      && (key === 'id' || key === 'params')) return false;
  if (parent.type === 'CatchClause' && key === 'param') return false;
  if (['LabeledStatement', 'BreakStatement', 'ContinueStatement'].includes(parent.type)) return false;
  if (parent.type === 'CallExpression' && key === 'callee') return false;
  if (parent.type === 'NewExpression' && key === 'callee') return false;
  return true;
}

// `document.body.style.width` → `document.body.style.width`;
// `t.style.width` (a local) → `.style.width`; `x.offsetWidth` → `.offsetWidth`.
function memberSurface(node, root, locals) {
  const path = pathOf(node);
  if (!path) return null;
  if (root && GLOBAL_OBJECTS.has(root) && !locals.has(root)) {
    return LANGUAGE.has(root) ? null : path;
  }
  const parts = path.split('.');
  // Keep `.style.<prop>` together; otherwise the last property.
  if (parts.length >= 3 && parts[parts.length - 2] === 'style') {
    return `.style.${parts[parts.length - 1]}`;
  }
  const last = parts[parts.length - 1];
  if (last === '*' || /^\d+$/.test(last)) return null;
  return `.${last}`;
}

function pathOf(node) {
  if (node.type === 'Identifier') return node.name;
  if (node.type === 'ThisExpression') return 'this';
  if (node.type === 'MemberExpression') {
    const object = pathOf(node.object);
    const property = node.computed
      ? (node.property.type === 'Literal' ? String(node.property.value) : '*')
      : node.property.name;
    return object ? `${object}.${property}` : `?.${property}`;
  }
  if (node.type === 'CallExpression') {
    const callee = pathOf(node.callee);
    return callee ? `${callee}()` : null;
  }
  return null;
}

function analyse(page) {
  const helpers = new Set(page.helpers);
  const programs = [];
  for (const script of page.scripts) {
    try {
      programs.push(parse(script.source, {
        ecmaVersion: 'latest', sourceType: 'script', allowReturnOutsideFunction: true,
      }));
    } catch (error) {
      return {file: page.file, parse_error: `${error.message}`, subtests: [], page_surfaces: []};
    }
  }
  const program = {type: 'Program', body: programs.flatMap(p => p.body)};
  const locals = declaredNames(program);
  // Only a top-level declaration can shadow the harness's `test` for the
  // whole page; a local `let test = async_test(...)` does not.
  const topNames = new Set();
  for (const statement of program.body) {
    if (statement.type === 'FunctionDeclaration' && statement.id) topNames.add(statement.id.name);
    if (statement.type === 'VariableDeclaration') {
      for (const d of statement.declarations) collectPattern(d.id, topNames);
    }
  }

  // Top-level named functions (declarations and `const f = () => ...`).
  const functions = new Map();
  const topLevel = [];
  for (const statement of program.body) {
    if (statement.type === 'FunctionDeclaration' && statement.id) {
      functions.set(statement.id.name, statement);
    } else if (statement.type === 'VariableDeclaration'
               && statement.declarations.every(d => d.init
                 && ['FunctionExpression', 'ArrowFunctionExpression'].includes(d.init.type))) {
      for (const d of statement.declarations) functions.set(d.id.name, d.init);
    } else {
      topLevel.push(statement);
    }
  }
  const functionSurfaces = new Map();
  for (const [name, node] of functions) functionSurfaces.set(name, surfacesOf(node, locals, helpers));
  const isDeclaration = node => (node.type === 'CallExpression' || node.type === 'NewExpression')
    && node.callee.type === 'Identifier'
    && ((HARNESS_DECLARATIONS.has(node.callee.name) && !topNames.has(node.callee.name))
        || (HELPER_DECLARATIONS.has(node.callee.name) && helpers.has(node.callee.name)));
  // The page's own top-level code, without the subtests it declares: each
  // declaration's body belongs to its own subtest.
  const pageLevel = surfacesOf({type: 'Program', body: topLevel}, locals, helpers, isDeclaration);

  // Functions that declare a subtest belong to that subtest: another
  // subtest that reaches one (through the page's test chain) does not take
  // its surfaces.
  const declaring = new Set();
  const findDeclaring = (node, enclosing) => {
    if (isDeclaration(node) && enclosing) declaring.add(enclosing);
    for (const [child] of children(node)) findDeclaring(child, enclosing);
  };
  for (const [name, node] of functions) findDeclaring(node, name);

  const reach = (start, own = null) => {
    const surfaces = new Set(start.surfaces);
    const reasons = new Set(start.reasons);
    const seen = new Set();
    const queue = [...start.calls];
    while (queue.length) {
      const name = queue.shift();
      if (seen.has(name) || !functionSurfaces.has(name)) continue;
      if (declaring.has(name) && name !== own) continue;
      seen.add(name);
      const found = functionSurfaces.get(name);
      found.surfaces.forEach(s => surfaces.add(s));
      found.reasons.forEach(r => reasons.add(r));
      queue.push(...found.calls);
    }
    return {surfaces, reasons};
  };

  // The argument index of the name of the subtest `node` declares, or null.
  const nameIndexOf = node => {
    const callee = node.callee.type === 'Identifier' ? node.callee.name : null;
    if (callee && HARNESS_DECLARATIONS.has(callee) && !topNames.has(callee)) {
      // test(fn, name), async_test(fn, name), async_test(name), promise_test(fn, name).
      const first = node.arguments[0];
      return first && ['FunctionExpression', 'ArrowFunctionExpression', 'Identifier',
        'CallExpression', 'MemberExpression'].includes(first.type)
        && constantString(first) === null ? 1 : 0;
    }
    if (callee && HELPER_DECLARATIONS.has(callee) && helpers.has(callee)) {
      return HELPER_DECLARATIONS.get(callee);
    }
    return null;
  };
  const declaredName = node => {
    const nameNode = node.arguments[nameIndexOf(node)];
    return nameNode === undefined ? page.title : constantString(nameNode);
  };

  // Variables holding an `async_test`, which a later link may finish.
  const asyncTests = new Map();
  const bindAsyncTest = (id, init) => {
    if (id?.type === 'Identifier' && init?.type === 'CallExpression' && isDeclaration(init)
        && init.callee.name === 'async_test') {
      const name = declaredName(init);
      if (name !== null) asyncTests.set(id.name, name);
    }
  };
  const findAsyncTests = node => {
    if (node.type === 'AssignmentExpression') bindAsyncTest(node.left, node.right);
    if (node.type === 'VariableDeclarator') bindAsyncTest(node.id, node.init);
    for (const [child] of children(node)) findAsyncTests(child);
  };
  findAsyncTests(program);

  // What a chain link runs. A synchronous `test()` reports its own failure
  // and the link goes on, so its body cannot stop the chain.
  const isSyncTest = node => node.type === 'CallExpression' && node.callee.type === 'Identifier'
    && node.callee.name === 'test' && !topNames.has('test');
  const gated = new Map();
  for (const [name, node] of functions) {
    gated.set(name, surfacesOf(node, locals, helpers, isSyncTest));
  }
  const linkOf = (body, label) => {
    const start = surfacesOf(body, locals, helpers, isSyncTest);
    const surfaces = new Set(start.surfaces);
    const reasons = new Set(start.reasons);
    const nodes = [body];
    const seen = new Set();
    const queue = [...start.calls];
    while (queue.length) {
      const name = queue.shift();
      if (seen.has(name) || !gated.has(name)) continue;
      seen.add(name);
      const found = gated.get(name);
      found.surfaces.forEach(s => surfaces.add(s));
      found.reasons.forEach(r => reasons.add(r));
      queue.push(...found.calls);
      nodes.push(functions.get(name));
    }
    const declared = new Set();
    const finishes = new Set();
    const walk = node => {
      if ((node.type === 'CallExpression' || node.type === 'NewExpression') && isDeclaration(node)) {
        const name = declaredName(node);
        if (name !== null && name !== undefined) declared.add(name);
      }
      if (node.type === 'CallExpression' && node.callee.type === 'MemberExpression'
          && propertyName(node.callee) === 'done' && node.callee.object.type === 'Identifier'
          && asyncTests.has(node.callee.object.name)) {
        finishes.add(asyncTests.get(node.callee.object.name));
      }
      for (const [child] of children(node)) walk(child);
    };
    nodes.forEach(walk);
    return {label, surfaces: [...surfaces].sort(), reasons: [...reasons].sort(),
      subtests: [...declared], finishes: [...finishes]};
  };
  // `f().then(() => g()).then(h)`, or null for anything else: a rejection
  // handler changes which links run.
  const chainOf = statement => {
    if (statement.type !== 'ExpressionStatement') return null;
    const bodies = [];
    let node = statement.expression;
    while (node.type === 'CallExpression' && node.callee.type === 'MemberExpression'
           && propertyName(node.callee) === 'then') {
      const handler = node.arguments[0];
      if (node.arguments.length !== 1) return null;
      if (['FunctionExpression', 'ArrowFunctionExpression'].includes(handler.type)) {
        bodies.unshift(handler.body);
      } else if (handler.type === 'Identifier') {
        bodies.unshift({type: 'CallExpression', callee: handler, arguments: []});
      } else {
        return null;
      }
      node = node.callee.object;
    }
    if (!bodies.length || node.type !== 'CallExpression' || node.callee.type !== 'Identifier'
        || !functions.has(node.callee.name)) return null;
    bodies.unshift(node);
    return bodies.map((body, index) => linkOf(body,
      body.type === 'CallExpression' && body.callee.type === 'Identifier'
        ? body.callee.name : `link${index}`));
  };
  const chains = topLevel.map(chainOf).filter(Boolean);

  // Find every subtest declaration and the top-level function enclosing it.
  const subtests = [];
  const visit = (node, enclosing, inLoop) => {
    if (node.type === 'CallExpression' || node.type === 'NewExpression') {
      const callee = node.callee.type === 'Identifier' ? node.callee.name : null;
      if (nameIndexOf(node) !== null) {
        const name = declaredName(node);
        const own = surfacesOf(node, locals, helpers);
        const base = enclosing ? functionSurfaces.get(enclosing) : {surfaces: new Set(),
          calls: new Set(), reasons: new Set()};
        const reached = reach({
          surfaces: new Set([...own.surfaces, ...base.surfaces, ...pageLevel.surfaces]),
          calls: new Set([...own.calls, ...base.calls, ...pageLevel.calls]),
          reasons: new Set([...own.reasons, ...base.reasons, ...pageLevel.reasons]),
        }, enclosing);
        const record = {
          name,
          name_known: name !== null && !inLoop,
          declared_by: `harness:${callee}`,
          surfaces: [...reached.surfaces].sort(),
        };
        const reasons = [...reached.reasons];
        if (inLoop) reasons.push('runtime-subtest-name');
        else if (name === null) reasons.push('runtime-subtest-name');
        if (reasons.length) record.untranslatable = [...new Set(reasons)].sort();
        subtests.push(record);
      }
    }
    for (const [child] of children(node)) {
      const loop = inLoop || ['ForStatement', 'ForInStatement', 'ForOfStatement',
        'WhileStatement', 'DoWhileStatement'].includes(node.type)
        || (node.type === 'CallExpression' && ['forEach', 'map'].includes(propertyName(node.callee)));
      visit(child, enclosing, loop);
    }
  };
  for (const statement of program.body) {
    if (statement.type === 'FunctionDeclaration' && statement.id) {
      visit(statement, statement.id.name, false);
    } else if (statement.type === 'VariableDeclaration' && functions.has(statement.declarations[0]?.id?.name)) {
      for (const d of statement.declarations) visit(d.init, d.id.name, false);
    } else {
      visit(statement, null, false);
    }
  }
  // A page that declares no subtest is a single-page test named by its title.
  if (subtests.length === 0) {
    const reached = reach(pageLevel);
    subtests.push({
      name: page.title, name_known: page.title !== null, declared_by: 'harness:single-page',
      surfaces: [...reached.surfaces].sort(),
      ...(reached.reasons.size ? {untranslatable: [...reached.reasons].sort()} : {}),
    });
  }
  return {file: page.file, subtests, page_surfaces: [...pageLevel.surfaces].sort(), chains};
}

// `--ast`: the parsed scripts themselves, one Program per inline script,
// for tools/wpt_script/codegen.py. A BigInt or RegExp literal's value is
// not JSON; codegen reads `raw` and reports them unsupported.
function programs(page) {
  try {
    return {file: page.file, programs: page.scripts.map(script => parse(script.source, {
      ecmaVersion: 'latest', sourceType: 'script', allowReturnOutsideFunction: true,
    }))};
  } catch (error) {
    return {file: page.file, parse_error: `${error.message}`, programs: []};
  }
}

const json = (key, value) => (typeof value === 'bigint' || value instanceof RegExp)
  ? String(value) : value;
process.stdout.write(process.argv[3] === '--ast'
  ? JSON.stringify(pages.map(programs), json) + '\n'
  : JSON.stringify(pages.map(analyse), null, 1) + '\n');
