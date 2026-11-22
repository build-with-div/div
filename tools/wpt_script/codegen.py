#!/usr/bin/env python3
"""Generate Div tests from the translatable WPT subtests (RFC 0005, step 3).

For each suite in GENERATED, every page with a subtest the ledger marks
translatable becomes one `#[test]` in `tests/wpt-script/tests/<module>.rs`:
the page's markup built with `divui` calls, its inline scripts compiled
from the pinned Acorn AST into Rust on `div_wpt_script::runtime`, and the
page's expected Div results from `tests/wpt-script/expected/`.

The compiler accepts a subset of JavaScript. Every other construct compiles
to a runtime `unsupported` with the construct's name, so a subtest that
reaches it reports UNSUPPORTED with that reason; nothing is approximated.

Usage:
  codegen.py                write the generated tests
  codegen.py --check        fail when they, or the expectations, are stale
  codegen.py --record DIR   record the results a test run wrote to DIR
                            (DIV_WPT_SCRIPT_RECORD=DIR cargo test -p
                            div-wpt-script) as the expectations, then write
"""

from __future__ import annotations

import argparse
import html.parser
import json
import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ledger  # noqa: E402

ROOT = ledger.ROOT
TOOL = ledger.TOOL
TESTS = ROOT / "tests/wpt-script/tests"
EXPECTED = ROOT / "tests/wpt-script/expected"
# The suites whose translatable subtests the runtime covers, and their
# generated module names. T3b-2 adds resize-observer.
GENERATED = {
    "html/webappapis/animation-frames": "animation_frames",
}
STATUSES = {"PASS": "Pass", "FAIL": "Fail", "TIMEOUT": "Timeout",
            "NOTRUN": "NotRun", "UNSUPPORTED": "Unsupported"}


# ----- Rust literals -----

class LoneSurrogate(Exception):
    pass


def rust_string(text: str) -> str:
    out = ['"']
    for ch in text:
        code = ord(ch)
        if 0xD800 <= code <= 0xDFFF:
            raise LoneSurrogate
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\t":
            out.append("\\t")
        elif code < 0x20 or code > 0x7E:
            out.append(f"\\u{{{code:x}}}")
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


def rust_number(value) -> str:
    value = float(value)
    if value != value:
        return "f64::NAN"
    if value in (float("inf"), float("-inf")):
        return "f64::INFINITY" if value > 0 else "f64::NEG_INFINITY"
    return repr(value)


def rust_ident(name: str) -> str:
    return "v_" + name.replace("$", "_dollar_")


def unsupported(reason: str) -> str:
    """An expression that stops the subtest: `reason` cannot be translated."""
    return f"rt.unsupported({rust_string(reason)})?"


# ----- scopes -----

FUNCTIONS = ("FunctionDeclaration", "FunctionExpression", "ArrowFunctionExpression")
LOOPS = ("ForStatement", "ForOfStatement", "ForInStatement", "WhileStatement",
         "DoWhileStatement")


def children(node):
    for key, value in node.items():
        if key in ("type", "start", "end", "loc", "range", "raw"):
            continue
        if isinstance(value, dict) and "type" in value:
            yield value
        elif isinstance(value, list):
            for item in value:
                if isinstance(item, dict) and "type" in item:
                    yield item


def identifiers(node, out=None):
    """Every identifier name in `node`, nested functions included."""
    if out is None:
        out = set()
    if node.get("type") == "Identifier":
        out.add(node["name"])
    for child in children(node):
        identifiers(child, out)
    return out


def references(node, name, count=0):
    if node.get("type") == "Identifier" and node["name"] == name:
        count += 1
    for child in children(node):
        count = references(child, name, count)
    return count


def strict(statements: list[dict]) -> bool:
    """Whether a directive prologue says "use strict"."""
    for statement in statements:
        if statement["type"] != "ExpressionStatement" or "directive" not in statement:
            return False
        if statement["directive"] == "use strict":
            return True
    return False


def uses_this(node) -> bool:
    """Whether `node` reads `this`, through arrows but not other functions."""
    if node.get("type") == "ThisExpression":
        return True
    if node.get("type") in ("FunctionDeclaration", "FunctionExpression"):
        return False
    return any(uses_this(child) for child in children(node))


def assigned(node, out=None) -> set[str]:
    """The identifiers `node` assigns, nested functions included."""
    if out is None:
        out = set()
    kind = node.get("type")
    if kind == "AssignmentExpression" and node["left"]["type"] == "Identifier":
        out.add(node["left"]["name"])
    if kind == "UpdateExpression" and node["argument"]["type"] == "Identifier":
        out.add(node["argument"]["name"])
    for child in children(node):
        assigned(child, out)
    return out


def contains_function(node) -> bool:
    return node.get("type") in FUNCTIONS or any(contains_function(c) for c in children(node))


class Scope:
    """One JavaScript function's variables (or the page's), all hoisted to
    the function: `var`, parameters and functions as plain cells, `let` and
    `const` as bindings with a temporal dead zone."""

    def __init__(self, parent: "Scope | None", arrow: bool = False):
        self.parent = parent
        self.arrow = arrow
        # Strict mode is inherited from the enclosing function or script.
        self.strict = parent.strict if parent is not None else False
        self.names: dict[str, str] = {}  # name -> var | let | const | function | param
        self.problem: str | None = None
        self.functions: list[dict] = []
        # Block id -> the let/const bindings declared directly in it, which
        # return to their temporal dead zone each time the block is entered.
        self.block_lexicals: dict[int, list[str]] = {}

    def lookup(self, name: str) -> str | None:
        scope = self
        while scope is not None:
            if name in scope.names:
                return scope.names[name]
            scope = scope.parent
        return None

    def visible(self) -> set[str]:
        names, scope = set(), self
        while scope is not None:
            names |= set(scope.names)
            scope = scope.parent
        return names

    def fail(self, reason: str):
        self.problem = self.problem or reason

    def declare(self, name: str, kind: str):
        existing = self.names.get(name)
        if existing is None:
            self.names[name] = kind
        elif existing in ("let", "const") or kind in ("let", "const"):
            self.fail("syntax:redeclaration")
        elif kind == "function":
            self.names[name] = "function"

    def collect(self, body: list[dict], function_root: dict):
        """Declare `body`'s variables (not nested functions')."""
        for statement in body:
            if statement["type"] == "FunctionDeclaration":
                self.declare(statement["id"]["name"], "function")
                self.functions.append(statement)
        for statement in body:
            self._walk(statement, depth=0, loop=False, function_root=function_root, block=None)

    def _walk(self, node, depth, loop, function_root, block):
        kind = node.get("type")
        if kind in FUNCTIONS:
            if kind == "FunctionDeclaration" and depth > 0:
                self.fail("syntax:block-function")
            return
        if kind == "ClassDeclaration":
            self.fail("syntax:ClassDeclaration")
            return
        if kind == "VariableDeclaration":
            for declarator in node["declarations"]:
                if declarator["id"]["type"] != "Identifier":
                    self.fail("syntax:destructuring")
                    continue
                name = declarator["id"]["name"]
                if node["kind"] == "var":
                    self.declare(name, "var")
                    continue
                if depth > 0:
                    # Hoisting a block's binding to the function is exact
                    # only when nothing outside the block names it.
                    if (self.parent is not None and name in self.parent.visible()) or \
                            references(function_root, name) != references(block, name):
                        self.fail("syntax:block-scoped-binding")
                    if loop and contains_function(block):
                        self.fail("syntax:per-iteration-binding")
                    self.block_lexicals.setdefault(id(block), []).append(name)
                self.declare(name, node["kind"])
        nested = kind in LOOPS or kind in ("BlockStatement", "SwitchStatement",
                                            "TryStatement", "CatchClause")
        for child in children(node):
            if nested:
                self._walk(child, depth + 1, loop or kind in LOOPS, function_root, node)
            else:
                self._walk(child, depth, loop, function_root, block)


# ----- the compiler -----

class Compiler:
    def __init__(self):
        self.temporary = 0
        self.label = 0

    def temp(self) -> str:
        self.temporary += 1
        return f"t{self.temporary}"

    # -- expressions --

    def expression(self, node, scope: Scope, name: str = "") -> str:
        method = getattr(self, "e_" + node["type"], None)
        if method is None:
            return unsupported("syntax:" + node["type"])
        try:
            return method(node, scope, name) if node["type"] in (
                "FunctionExpression", "ArrowFunctionExpression") else method(node, scope)
        except LoneSurrogate:
            return unsupported("lone-surrogate")

    def e_Literal(self, node, scope):
        value = node.get("value")
        if "regex" in node:
            return unsupported("syntax:regex")
        if "bigint" in node:
            return unsupported("syntax:bigint")
        if value is None:
            return "Value::Null"
        if isinstance(value, bool):
            return f"Value::Bool({'true' if value else 'false'})"
        if isinstance(value, (int, float)):
            return f"Value::Number({rust_number(value)})"
        return f"Value::str({rust_string(value)})"

    def e_TemplateLiteral(self, node, scope):
        parts = []
        for index, quasi in enumerate(node["quasis"]):
            cooked = quasi["value"].get("cooked")
            if cooked is None:
                return unsupported("syntax:invalid-template-escape")
            if cooked:
                parts.append(f"text.push_str({rust_string(cooked)});")
            if index < len(node["expressions"]):
                value, t = self.expression(node["expressions"][index], scope), self.temp()
                parts.append(f"let {t} = rt.to_string(&{value})?; text.push_str(&{t});")
        return "{ let mut text = String::new(); " + " ".join(parts) + " Value::str(&text) }"

    def e_Identifier(self, node, scope):
        name = node["name"]
        kind = scope.lookup(name)
        if kind is None:
            return f"rt.global({rust_string(name)})?"
        if kind in ("let", "const"):
            return f"read(&{rust_ident(name)}, {rust_string(name)})?"
        return f"load(&{rust_ident(name)})"

    def e_ThisExpression(self, node, scope):
        return "this_.clone()"

    def store(self, target, value: str, scope: Scope) -> str:
        """A statement assigning the Rust value expression `value`."""
        if target["type"] == "Identifier":
            name = target["name"]
            kind = scope.lookup(name)
            if kind is None:
                return f"{unsupported('global-assignment:' + name)};"
            if kind == "const":
                return ('return Err(Throw::error('
                        '"TypeError: Assignment to constant variable."));')
            if kind == "function-name":
                # A named function expression's own name is immutable:
                # assigning it throws in strict mode and does nothing in
                # sloppy mode.
                if scope.strict:
                    return ('return Err(Throw::error('
                            '"TypeError: Assignment to constant variable."));')
                return f"let _ = {value};"
            if kind == "let":
                return f"write(&{rust_ident(name)}, {rust_string(name)}, {value})?;"
            return f"store(&{rust_ident(name)}, {value});"
        return f"{unsupported('syntax:assignment-target')};"

    def member_parts(self, node, scope):
        """Rust statements binding the object to `o` and the key to `k`."""
        o, k = self.temp(), self.temp()
        lines = [f"let {o} = {self.expression(node['object'], scope)};"]
        if node.get("computed"):
            lines.append(f"let {k} = rt.key(&{self.expression(node['property'], scope)})?;")
        elif node["property"]["type"] == "PrivateIdentifier":
            return None
        else:
            lines.append(f"let {k} = {rust_string(node['property']['name'])}.to_owned();")
        return o, k, lines

    def e_MemberExpression(self, node, scope):
        if node.get("optional"):
            return unsupported("syntax:optional-chaining")
        if node["object"]["type"] == "Super":
            return unsupported("syntax:super")
        parts = self.member_parts(node, scope)
        if parts is None:
            return unsupported("syntax:private-field")
        o, k, lines = parts
        return "{ " + " ".join(lines) + f" rt.get(&{o}, &{k})? }}"

    def e_ChainExpression(self, node, scope):
        return unsupported("syntax:optional-chaining")

    def arguments(self, nodes, scope):
        if any(node["type"] == "SpreadElement" for node in nodes):
            return None
        return "vec![" + ", ".join(self.expression(node, scope) for node in nodes) + "]"

    def e_CallExpression(self, node, scope):
        if node.get("optional"):
            return unsupported("syntax:optional-chaining")
        callee = node["callee"]
        if callee["type"] == "Super":
            return unsupported("syntax:super")
        if callee["type"] == "MemberExpression":
            if callee.get("optional") or callee["object"]["type"] == "Super":
                return unsupported("syntax:optional-chaining")
            parts = self.member_parts(callee, scope)
            if parts is None:
                return unsupported("syntax:private-field")
            o, k, lines = parts
            arguments = self.arguments(node["arguments"], scope)
            if arguments is None:
                return unsupported("syntax:spread")
            a = self.temp()
            return ("{ " + " ".join(lines) + f" let {a} = {arguments};"
                    f" rt.call_method(&{o}, &{k}, {a})? }}")
        f, a = self.temp(), self.temp()
        arguments = self.arguments(node["arguments"], scope)
        if arguments is None:
            return unsupported("syntax:spread")
        return (f"{{ let {f} = {self.expression(callee, scope)}; let {a} = {arguments};"
                f" rt.call(&{f}, Value::Undefined, {a})? }}")

    def e_NewExpression(self, node, scope):
        f, a = self.temp(), self.temp()
        arguments = self.arguments(node["arguments"], scope)
        if arguments is None:
            return unsupported("syntax:spread")
        return (f"{{ let {f} = {self.expression(node['callee'], scope)}; let {a} = {arguments};"
                f" rt.construct(&{f}, {a})? }}")

    def e_AssignmentExpression(self, node, scope):
        operator, target = node["operator"], node["left"]
        if operator in ("&&=", "||=", "??="):
            return unsupported("syntax:logical-assignment")
        binary = operator[:-1]
        if target["type"] == "MemberExpression":
            if target.get("optional") or target["object"]["type"] == "Super":
                return unsupported("syntax:assignment-target")
            parts = self.member_parts(target, scope)
            if parts is None:
                return unsupported("syntax:private-field")
            o, k, lines = parts
            v = self.temp()
            right = self.expression(node["right"], scope)
            if binary:
                c, r = self.temp(), self.temp()
                lines += [f"let {c} = rt.get(&{o}, &{k})?;", f"let {r} = {right};",
                          f"let {v} = rt.binary({rust_string(binary)}, &{c}, &{r})?;"]
            else:
                lines.append(f"let {v} = {right};")
            lines.append(f"rt.set(&{o}, &{k}, {v}.clone())?;")
            return "{ " + " ".join(lines) + f" {v} }}"
        if target["type"] != "Identifier":
            return unsupported("syntax:destructuring")
        v = self.temp()
        right = self.expression(node["right"], scope, target["name"])
        if binary:
            c, r = self.temp(), self.temp()
            lines = [f"let {c} = {self.e_Identifier(target, scope)};", f"let {r} = {right};",
                     f"let {v} = rt.binary({rust_string(binary)}, &{c}, &{r})?;"]
        else:
            lines = [f"let {v} = {right};"]
        lines.append(self.store(target, f"{v}.clone()", scope))
        return "{ " + " ".join(lines) + f" {v} }}"

    def e_UpdateExpression(self, node, scope):
        target = node["argument"]
        delta = "1.0" if node["operator"] == "++" else "-1.0"
        old, new = self.temp(), self.temp()
        result = new if node["prefix"] else f"Value::Number({old})"
        if target["type"] == "Identifier":
            lines = [f"let {old} = rt.to_number(&{self.e_Identifier(target, scope)})?;",
                     f"let {new} = Value::Number({old} + {delta});",
                     self.store(target, f"{new}.clone()", scope)]
            return "{ " + " ".join(lines) + f" {result} }}"
        if target["type"] == "MemberExpression" and not target.get("optional"):
            parts = self.member_parts(target, scope)
            if parts is None:
                return unsupported("syntax:private-field")
            o, k, lines = parts
            lines += [f"let {old} = rt.to_number(&rt.get(&{o}, &{k})?)?;",
                      f"let {new} = Value::Number({old} + {delta});",
                      f"rt.set(&{o}, &{k}, {new}.clone())?;"]
            return "{ " + " ".join(lines) + f" {result} }}"
        return unsupported("syntax:update-target")

    def e_UnaryExpression(self, node, scope):
        operator, argument = node["operator"], node["argument"]
        if operator == "typeof" and argument["type"] == "Identifier" and \
                scope.lookup(argument["name"]) is None:
            return f"rt.type_of_global({rust_string(argument['name'])})?"
        value = self.expression(argument, scope)
        if operator == "!":
            return f"rt.not(&{value})"
        if operator == "-":
            return f"rt.negate(&{value})?"
        if operator == "+":
            return f"Value::Number(rt.to_number(&{value})?)"
        if operator == "typeof":
            return f"rt.type_of(&{value})?"
        if operator == "void":
            return f"{{ let _ = {value}; Value::Undefined }}"
        return unsupported("syntax:operator " + operator)

    def e_BinaryExpression(self, node, scope):
        if node["left"]["type"] == "PrivateIdentifier":
            return unsupported("syntax:private-field")
        a, b = self.temp(), self.temp()
        return (f"{{ let {a} = {self.expression(node['left'], scope)};"
                f" let {b} = {self.expression(node['right'], scope)};"
                f" rt.binary({rust_string(node['operator'])}, &{a}, &{b})? }}")

    def e_LogicalExpression(self, node, scope):
        a = self.temp()
        left = self.expression(node["left"], scope)
        right = self.expression(node["right"], scope)
        condition = {
            "&&": f"rt.truthy(&{a})",
            "||": f"!rt.truthy(&{a})",
            "??": f"matches!({a}, Value::Undefined | Value::Null)",
        }[node["operator"]]
        return f"{{ let {a} = {left}; if {condition} {{ {right} }} else {{ {a} }} }}"

    def e_ConditionalExpression(self, node, scope):
        return (f"{{ if rt.truthy(&{self.expression(node['test'], scope)}) "
                f"{{ {self.expression(node['consequent'], scope)} }} "
                f"else {{ {self.expression(node['alternate'], scope)} }} }}")

    def e_SequenceExpression(self, node, scope):
        *init, last = node["expressions"]
        return "{ " + " ".join(f"let _ = {self.expression(e, scope)};" for e in init) + \
            f" {self.expression(last, scope)} }}"

    def e_ArrayExpression(self, node, scope):
        if any(item is None or item["type"] == "SpreadElement" for item in node["elements"]):
            return unsupported("syntax:array-hole-or-spread")
        return "Value::array(vec![" + ", ".join(
            self.expression(item, scope) for item in node["elements"]) + "])"

    def e_ObjectExpression(self, node, scope):
        entries = []
        for prop in node["properties"]:
            if prop["type"] != "Property" or prop["computed"] or prop["kind"] != "init" \
                    or prop["method"]:
                return unsupported("syntax:object-literal-form")
            key = prop["key"]
            if key["type"] == "Identifier":
                text = key["name"]
            elif key["type"] == "Literal" and isinstance(key["value"], str):
                text = key["value"]
            else:
                return unsupported("syntax:object-literal-key")
            if any(text == existing for existing, _ in entries):
                return unsupported("syntax:duplicate-key")
            entries.append((text, self.expression(prop["value"], scope, text)))
        return "Value::object(vec![" + ", ".join(
            f"({rust_string(k)}, {v})" for k, v in entries) + "])"

    def e_FunctionExpression(self, node, scope, name=""):
        return self.function(node, scope, name)

    def e_ArrowFunctionExpression(self, node, scope, name=""):
        return self.function(node, scope, name)

    def function(self, node, outer: Scope, name: str) -> str:
        if node.get("async") or node.get("generator"):
            return unsupported("syntax:async-or-generator")
        arrow = node["type"] == "ArrowFunctionExpression"
        if node.get("id"):
            name = node["id"]["name"]
        # A named function expression's own name is visible inside it only.
        self_name = node["id"]["name"] if node["type"] == "FunctionExpression" and node.get("id") \
            else None
        if self_name:
            outer = Scope(outer)
            outer.names[self_name] = "function-name"
        scope = Scope(outer, arrow=arrow)
        lines = []
        for index, param in enumerate(node["params"]):
            if param["type"] != "Identifier":
                scope.fail("syntax:pattern-parameter")
                continue
            scope.declare(param["name"], "param")
        body = node["body"]
        statements = body["body"] if body["type"] == "BlockStatement" else None
        if statements is not None:
            scope.collect(statements, body)
        else:
            scope._walk(body, 0, False, body, None)
        uses_arguments = not arrow and "arguments" in identifiers(body) and \
            "arguments" not in scope.names
        if statements is not None and strict(statements):
            scope.strict = True
        # A sloppy function's `arguments` aliases its parameters.
        if uses_arguments and not scope.strict and \
                assigned(body) & {p["name"] for p in node["params"]
                                  if p["type"] == "Identifier"}:
            scope.fail("syntax:mapped-arguments")
        captures = sorted((identifiers(node) & outer.visible()) - set(scope.names))
        prelude = [f"let {rust_ident(c)} = {rust_ident(c)}.clone();" for c in captures]
        if arrow:
            prelude.append("let this_ = this_.clone();")
        this_parameter = "_this" if arrow else "this_"
        if scope.problem:
            lines.append(f"return rt.unsupported({rust_string(scope.problem)});")
        else:
            if not arrow and not scope.strict and uses_this(body):
                # Sloppy mode: a missing `this` is the global object, and a
                # primitive one would be boxed (not translated).
                lines.append("let this_ = sloppy_this(this_)?;")
            for index, param in enumerate(node["params"]):
                lines.append(f"let {rust_ident(param['name'])} = "
                             f"cell(args.get({index}).cloned().unwrap_or_default());")
            if uses_arguments:
                scope.names["arguments"] = "param"
                lines.append(f"let {rust_ident('arguments')} = cell(Value::array(args.clone()));")
            lines += self.declarations(scope)
            if statements is not None:
                lines += self.hoisted_functions(scope)
                for statement in statements:
                    lines += self.statement(statement, scope)
                lines.append("Ok(Value::Undefined)")
            else:
                lines.append(f"Ok({self.expression(body, scope)})")
        inner = "\n".join(lines)
        function = (f"move |rt: &Rt, {this_parameter}: Value, args: Vec<Value>| -> Res {{\n"
                    f"{inner}\n}}")
        value = f"Value::function({rust_string(name)}, {{ {' '.join(prelude)} {function} }})"
        if self_name:
            # `function f() { ... f ... }` as an expression: bind its own name.
            cell_name = rust_ident(self_name)
            return (f"{{ let {cell_name} = cell(Value::Undefined); let function = {value};"
                    f" store(&{cell_name}, function.clone()); function }}")
        return value

    def declarations(self, scope: Scope) -> list[str]:
        lines = []
        for name, kind in scope.names.items():
            if kind == "param":
                continue
            if kind in ("let", "const"):
                lines.append(f"let {rust_ident(name)} = binding();")
            elif kind != "param":
                lines.append(f"let {rust_ident(name)} = cell(Value::Undefined);")
        return lines

    def hoisted_functions(self, scope: Scope, functions=None) -> list[str]:
        lines = []
        for declaration in functions if functions is not None else scope.functions:
            name = declaration["id"]["name"]
            value = self.function(declaration, scope, name)
            lines.append(f"store(&{rust_ident(name)}, {value});")
        return lines

    # -- statements --

    def resets(self, node, scope: Scope) -> list[str]:
        """Return the block's let/const bindings to their dead zone."""
        return [f"store(&{rust_ident(name)}, Value::Uninitialized);"
                for name in scope.block_lexicals.get(id(node), [])]

    def statement(self, node, scope: Scope, loop: str | None = None) -> list[str]:
        kind = node["type"]
        if kind == "ExpressionStatement":
            return [f"let _ = {self.expression(node['expression'], scope)};"]
        if kind == "VariableDeclaration":
            lines = []
            for declarator in node["declarations"]:
                target = declarator["id"]
                if target["type"] != "Identifier":
                    return [f"return rt.unsupported({rust_string('syntax:destructuring')});"]
                ident = rust_ident(target["name"])
                if declarator.get("init") is not None:
                    value = self.expression(declarator["init"], scope, target["name"])
                    lines.append(f"store(&{ident}, {value});")
                elif node["kind"] != "var":
                    lines.append(f"store(&{ident}, Value::Undefined);")
            return lines
        if kind == "FunctionDeclaration":
            return []  # hoisted
        if kind == "EmptyStatement":
            return []
        if kind == "ReturnStatement":
            argument = node.get("argument")
            value = self.expression(argument, scope) if argument else "Value::Undefined"
            return [f"return Ok({value});"]
        if kind == "BlockStatement":
            return ["{"] + self.resets(node, scope) + [line for s in node["body"] for line in self.statement(s, scope, loop)] \
                + ["}"]
        if kind == "IfStatement":
            lines = [f"if rt.truthy(&{self.expression(node['test'], scope)}) {{"]
            lines += self.statement(node["consequent"], scope, loop)
            if node.get("alternate"):
                lines += ["} else {"] + self.statement(node["alternate"], scope, loop)
            return lines + ["}"]
        if kind in ("BreakStatement", "ContinueStatement"):
            if node.get("label") or loop is None:
                return [f"return rt.unsupported({rust_string('syntax:labelled-jump')});"]
            return [f"break 'l{loop};" if kind == "BreakStatement" else f"break 'c{loop};"]
        if kind in ("ForStatement", "WhileStatement", "DoWhileStatement"):
            self.label += 1
            label = str(self.label)
            lines = self.resets(node, scope)
            if kind == "ForStatement" and node.get("init"):
                init = node["init"]
                lines += (self.statement(init, scope) if init["type"] == "VariableDeclaration"
                          else [f"let _ = {self.expression(init, scope)};"])
            test = node.get("test")
            check = (f"if !rt.truthy(&{self.expression(test, scope)}) {{ break 'l{label}; }}"
                     if test else "")
            body = self.statement(node["body"], scope, label)
            update = (f"let _ = {self.expression(node['update'], scope)};"
                      if kind == "ForStatement" and node.get("update") else "")
            if kind == "DoWhileStatement":
                lines += [f"'l{label}: loop {{", f"'c{label}: {{"] + body + ["}", check, "}"]
            else:
                lines += [f"'l{label}: loop {{", check, f"'c{label}: {{"] + body + \
                    ["}", update, "}"]
            return ["{"] + lines + ["}"]
        if kind == "ForOfStatement":
            if node.get("await"):
                return [f"return rt.unsupported({rust_string('syntax:for-await')});"]
            left = node["left"]
            if left["type"] == "VariableDeclaration":
                if len(left["declarations"]) != 1 or \
                        left["declarations"][0]["id"]["type"] != "Identifier":
                    return [f"return rt.unsupported({rust_string('syntax:destructuring')});"]
                target = left["declarations"][0]["id"]
            elif left["type"] == "Identifier":
                target = left
            else:
                return [f"return rt.unsupported({rust_string('syntax:destructuring')});"]
            self.label += 1
            label = str(self.label)
            item = self.temp()
            assign = (f"store(&{rust_ident(target['name'])}, {item});"
                      if left["type"] == "VariableDeclaration"
                      else self.store(target, item, scope))
            return [f"'l{label}: for {item} in rt.iterate(&{self.expression(node['right'], scope)})? {{",
                    assign, f"'c{label}: {{"] + self.statement(node["body"], scope, label) + \
                ["}", "}"]
        if kind == "ThrowStatement":
            # A thrown value would need catch and the harness's message
            # formatting of any value; T3b translates neither yet.
            return [f"return rt.unsupported({rust_string('syntax:ThrowStatement')});"]
        return [f"return rt.unsupported({rust_string('syntax:' + kind)});"]


# ----- markup -----

VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
        "source", "track", "wbr"}
IGNORED = {"html", "head", "body", "title", "meta", "link"}


class Markup(html.parser.HTMLParser):
    """The page's elements, text and inline scripts in document order."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.events: list[tuple] = []
        self.stack: list[int | None] = []
        self.count = 0
        self.scripts = 0
        self.in_script = False
        self.in_style = False
        self.in_title = False

    def parent(self):
        for item in reversed(self.stack):
            if item is not None:
                return item
        return None

    def handle_starttag(self, tag, attrs):
        tag = tag.split(":")[-1]
        if tag == "script":
            source = dict(attrs).get("src")
            if source is None:
                self.events.append(("script", self.scripts))
                self.scripts += 1
            elif source not in ledger.HARNESS_SCRIPTS:
                self.events.append(("unsupported", f"intrinsic:{source}"))
            self.in_script = True
            return
        if tag == "style":
            self.events.append(("unsupported", "markup:<style>"))
            self.in_style = True
            return
        if tag == "link" and any(k == "rel" and v and "stylesheet" in v.lower()
                                 for k, v in attrs):
            self.events.append(("unsupported", "markup:stylesheet"))
            return
        if tag in ("html", "body") and attrs:
            self.events.append(("unsupported", f"markup:{tag}-attributes"))
        if tag == "title":
            self.in_title = True
        if tag in IGNORED:
            if tag not in VOID:
                self.stack.append(None)
            return
        self.count += 1
        self.events.append(("element", self.count, tag,
                            [(k, v if v is not None else "") for k, v in attrs], self.parent()))
        if tag not in VOID:
            self.stack.append(self.count)

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)
        if tag.split(":")[-1] not in VOID:
            self.handle_endtag(tag)

    def handle_endtag(self, tag):
        tag = tag.split(":")[-1]
        if tag == "script":
            self.in_script = False
            return
        if tag == "title":
            self.in_title = False
        if tag == "style":
            self.in_style = False
            return
        if tag in VOID:
            return
        if self.stack:
            self.stack.pop()

    def handle_data(self, data):
        if self.in_script or self.in_style or self.in_title or not data.strip():
            return
        self.events.append(("text", data, self.parent()))


# ----- pages -----

def test_name(relative: str, suite: str, used: set[str]) -> str:
    stem = relative[len(suite) + 1:].rsplit(".", 1)[0]
    name = re.sub(r"[^0-9a-zA-Z]+", "_", stem).strip("_").lower()
    if not name or name[0].isdigit():
        name = "page_" + name
    base, index = name, 2
    while name in used:
        name, index = f"{base}_{index}", index + 1
    used.add(name)
    return name


def page_test(relative: str, title: str | None, text: str, programs: list[dict],
              names: list[str], expected: dict | None, compiler: Compiler) -> list[str]:
    markup = Markup()
    markup.feed(text)
    markup.close()
    if markup.scripts != len(programs):
        raise SystemExit(f"{relative}: {markup.scripts} inline scripts in the markup, "
                         f"{len(programs)} parsed")
    page = Scope(None)
    for program in programs:
        page.collect(program["body"], program)
    lines = ["let this_ = Value::Host(Host::Window);"]
    if page.problem:
        lines.append(f"rt.unsupported_page({rust_string(page.problem)});")
        lines.append("return;")
    else:
        lines += compiler.declarations(page)
        for event in markup.events:
            if event[0] == "unsupported":
                lines.append(f"rt.unsupported_page({rust_string(event[1])});")
                lines.append("return;")
                break
            if event[0] == "element":
                _, index, tag, attrs, parent = event
                pairs = ", ".join(f"({rust_string(k)}, {rust_string(v)})" for k, v in attrs)
                where = f"Some(&e{parent})" if parent else "None"
                lines.append(f"let e{index} = match rt.element({rust_string(tag)}, &[{pairs}], "
                             f"{where}) {{ Ok(e) => e, Err(error) => {{ "
                             f"rt.unsupported_page(&format!(\"{{error:?}}\")); return; }} }};")
            elif event[0] == "text":
                _, data, parent = event
                where = f"Some(&e{parent})" if parent else "None"
                lines.append(f"if let Err(error) = rt.text({rust_string(data)}, {where}) {{ "
                             f"rt.unsupported_page(&format!(\"{{error:?}}\")); return; }}")
            else:
                program = programs[event[1]]
                # Each script has its own directive prologue.
                page.strict = strict(program["body"])
                functions = [s for s in program["body"] if s["type"] == "FunctionDeclaration"]
                body = compiler.hoisted_functions(page, functions)
                for statement in program["body"]:
                    body += compiler.statement(statement, page)
                lines.append("if let Err(thrown) = (|| -> Res {")
                lines += body
                lines += ["Ok(Value::Undefined)", "})() {", "rt.uncaught(thrown);", "}"]
    rows = []
    for name in names:
        status = (expected or {}).get("subtests", {}).get(name)
        if status is not None:
            rows.append(f"({rust_string(name)}, Expected::{STATUSES[status['status']]})")
    harness = (expected or {}).get("harness_error")
    return [
        f"    let rt = Rt::new({'Some(' + rust_string(title) + ')' if title is not None else 'None'});",
        "    let result = rt.run_page(|rt| {",
        *indent(lines, 2),
        "    });",
        f"    expect({rust_string(relative)}, &result, "
        f"{'None' if harness is None else 'Some(' + rust_string(harness) + ')'}, &[",
        *[f"        {row}," for row in rows],
        "    ]);",
    ]


def indent(lines: list[str], level: int) -> list[str]:
    out, depth = [], level
    for line in lines:
        for part in line.split("\n"):
            stripped = part.strip()
            if stripped.startswith("}") or stripped.startswith(")"):
                depth = max(level, depth - 1)
            out.append("    " * depth + stripped if stripped else "")
            opens = stripped.count("{") - stripped.count("}")
            if stripped.startswith("}"):
                opens += 1
            depth = max(level, depth + opens)
    return out


HEADER = """\
// Generated by tools/wpt_script/codegen.py from WPT {suite} at Chromium
// {chromium} ({revision}). Do not edit: regenerate (RFC 0005).
#![allow(unused, unreachable_code, unused_labels, non_snake_case, clippy::all)]

use div_wpt_script::runtime::*;
"""


def generate(suite: str, module: str, wpt_root: Path, acorn: str, intrinsics: dict,
             expected: dict) -> tuple[str, list[str]]:
    current = ledger.load_json(ledger.ledger_path(suite))
    translatable: dict[str, list[str]] = {}
    for row in current["rows"]:
        if row["status"] == "translatable":
            translatable.setdefault(row["file"], []).append(row["name"])
    files = sorted(translatable)
    pages = [ledger.read_page(wpt_root, relative, intrinsics) for relative in files]
    payload = [{"file": p["file"], "title": p["title"], "scripts": p["scripts"],
                "helpers": []} for p in pages]
    result = subprocess.run(
        ["node", str(TOOL / "translate.mjs"), acorn, "--ast"], input=json.dumps(payload),
        capture_output=True, text=True, encoding="utf-8", check=True)
    parsed = {record["file"]: record for record in json.loads(result.stdout)}
    compiler = Compiler()
    out = [HEADER.format(suite=suite, chromium=current["chromium"],
                         revision=current["wpt_revision"]).rstrip("\n")]
    missing, used = [], set()
    for page in pages:
        relative = page["file"]
        if "parse_error" in parsed[relative]:
            raise SystemExit(f"{relative}: {parsed[relative]['parse_error']}")
        page_expected = expected.get(relative)
        for name in translatable[relative]:
            if (page_expected or {}).get("subtests", {}).get(name) is None:
                missing.append(f"{relative}: {name}")
        text = (wpt_root / relative).read_text(encoding="utf-8")
        out.append("")
        out.append("#[test]")
        out.append("#[rustfmt::skip]")
        out.append(f"fn {test_name(relative, suite, used)}() {{")
        out += page_test(relative, page["title"], text, parsed[relative]["programs"],
                         translatable[relative], page_expected, compiler)
        out.append("}")
    return "\n".join(out) + "\n", missing


def expected_path(suite: str) -> Path:
    return EXPECTED / (suite.replace("/", "--") + ".json")


def record(suite: str, directory: Path, expected: dict) -> dict:
    """The expectations from a recording run's results for `suite`."""
    current = ledger.load_json(ledger.ledger_path(suite))
    translatable: dict[str, list[str]] = {}
    for row in current["rows"]:
        if row["status"] == "translatable":
            translatable.setdefault(row["file"], []).append(row["name"])
    results = {}
    for path in sorted(directory.glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        results[data["page"]] = data
    updated = {}
    for relative, names in sorted(translatable.items()):
        data = results.get(relative)
        if data is None:
            if relative in expected:
                updated[relative] = expected[relative]
            continue
        observed = {row["name"]: row for row in data["subtests"]}
        subtests = {}
        for name in names:
            row = observed.get(name)
            if row is None:
                row = ({"status": "UNSUPPORTED", "message": data["unsupported"]}
                       if data.get("unsupported") else {"status": "NOTRUN"})
            subtests[name] = {k: row[k] for k in ("status", "message") if k in row}
        updated[relative] = {"harness_error": data.get("harness_error"), "subtests": subtests}
    return updated


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--wpt-root", type=Path, default=os.environ.get("WPT_SCRIPT_ROOT"))
    parser.add_argument("--acorn", default=os.environ.get("CHROMIUM_ACORN"))
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--record", type=Path)
    args = parser.parse_args()
    if not args.wpt_root or not args.acorn:
        raise SystemExit("set --wpt-root/WPT_SCRIPT_ROOT and --acorn/CHROMIUM_ACORN")
    if ledger.sha256(Path(args.acorn)) != ledger.ACORN_SHA256:
        raise SystemExit(f"{args.acorn} is not the pinned acorn@8.14.0")
    intrinsics = ledger.load_json(TOOL / "intrinsics.json")
    failed = False
    for suite, module in GENERATED.items():
        path = expected_path(suite)
        expected = ledger.load_json(path) if path.is_file() else {}
        if args.record:
            expected = record(suite, args.record, expected)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(expected, indent=1, ensure_ascii=False) + "\n",
                            encoding="utf-8", newline="\n")
            print(path.relative_to(ROOT))
        text, missing = generate(suite, module, args.wpt_root.resolve(), args.acorn,
                                 intrinsics, expected)
        target = TESTS / f"{module}.rs"
        if missing:
            print(f"{suite}: no recorded Div result for {len(missing)} translatable "
                  f"subtests (run the tests with DIV_WPT_SCRIPT_RECORD, then --record):",
                  file=sys.stderr)
            for line in missing[:10]:
                print(f"  {line}", file=sys.stderr)
            failed = failed or args.check
        if args.check:
            if not target.is_file() or target.read_text(encoding="utf-8") != text:
                print(f"{target.relative_to(ROOT)} is stale", file=sys.stderr)
                failed = True
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text, encoding="utf-8", newline="\n")
            print(target.relative_to(ROOT))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
