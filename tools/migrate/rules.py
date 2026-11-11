"""Rules that turn an Open UI tree into the Div tree.

Every tracked source path must match exactly one PATH_RULES entry; the first
match wins and an unmatched path is an error, so nothing is moved, dropped or
kept by accident.

Actions:
  ("main", replacement)  keep in the Div repository; `replacement` is an
                         re.sub template applied to the whole old path.
  ("gates",)             data repository, profile `gates` (CI reads these).
  ("archive",)           data repository, profile `archive` (read by no gate).
  ("drop",)              not carried over; it stays in the archived repository.

Data-repository files keep their old repository-relative path, and
tools/fidelity_data.py restores them at that path, so tools and the frozen,
hash-pinned ledgers that name those paths keep working unchanged.
"""

CRATES = [
    "geometry", "style", "style-derive", "dom", "layout", "paint", "compositor",
    "engine", "platform", "ffi", "text", "macros", "cli",
]
_CR = "|".join(sorted(CRATES, key=len, reverse=True))

# Files the Div workspace regenerates or keeps under the old data paths.
LIVE_RENDERER_LEDGERS = (
    "qualification-contract-v2.json",
    "author-style-inventory.json",
    "javascript-mutation-audit-v4.json",
)

PATH_RULES = [
    # ---- legacy C++/Blink backend and its build: archive only -------------
    (r"^(src|build|test_renders|\.specify)/.*$", ("drop",)),
    (r"^(\.gn|BUILD\.gn|BUILDCONFIG\.gn)$", ("drop",)),
    (r"^include/openui/.*$", ("drop",)),
    (r"^examples/(BUILD\.gn|hello_world\.cc|skia_gallery\.c|skia_hello\.c|skia_poc/.*)$", ("drop",)),
    (r"^tests/.*$", ("drop",)),
    (r"^bindings/rust/openui-(sys|build)/.*$", ("drop",)),
    (r"^bindings/rust/\.cargo/config\.chromium\.toml$", ("drop",)),
    # ---- root clutter ------------------------------------------------------
    (r"^(\.copilot-summary-backup\.json|nohup\.out|test_templates_dump\.txt|test_cpu_raster\.png)$", ("drop",)),
    (r"^(diff|openui)_probe_010(_ref)?\.png$", ("drop",)),
    (r"^result_probe_010(_ref)?\.json$", ("drop",)),
    (r"^package(-lock)?\.json$", ("drop",)),
    # Spec-Kit agent prompts and the Copilot memory instructions.
    (r"^\.github/(agents|prompts)/.*$", ("drop",)),
    (r"^\.github/copilot-instructions\.md$", ("drop",)),
    # The README and AGENTS.md are rewritten (overlay/).
    (r"^(README|AGENTS)\.md$", ("drop",)),
    # ---- root files kept ---------------------------------------------------
    (r"^(\.clang-format|\.gitattributes|\.gitignore|CHANGELOG\.md|CHROMIUM_VERSION|CONTRIBUTING\.md|LICENSE)$", ("main", r"\g<0>")),
    (r"^brand/.*$", ("main", r"\g<0>")),
    (r"^site/.*$", ("main", r"\g<0>")),
    (r"^\.github/workflows/release-v02\.yml$", ("main", ".github/workflows/release.yml")),
    (r"^\.github/workflows/.*$", ("main", r"\g<0>")),
    # ---- the Rust workspace ------------------------------------------------
    (r"^bindings/rust/openui/(.*)$", ("main", r"crates/divui/\1")),
    (rf"^bindings/rust/openui-({_CR})/(.*)$", ("main", r"crates/div-\1/\2")),
    (r"^bindings/rust/examples/(.*)$", ("main", r"examples/rust/\1")),
    (r"^bindings/rust/pixel-compare/(.*)$", ("main", r"tests/fidelity/pixel-compare/\1")),
    (r"^bindings/rust/(vendor|fuzz|\.cargo)/(.*)$", ("main", r"\1/\2")),
    (r"^bindings/rust/(Cargo\.toml|Cargo\.lock)$", ("main", r"\1")),
    # ---- components: one workspace with the framework ----------------------
    (r"^components/(Cargo\.toml|Cargo\.lock)$", ("drop",)),
    (r"^components/openui-components/(.*)$", ("main", r"crates/div-components/\1")),
    (r"^components/(README\.md|THIRD_PARTY\.md)$", ("main", r"crates/div-components/\1")),
    # The committed reference pages load ../../assets/: keep the area's layout.
    (r"^components/(assets/.*|\.gitattributes)$", ("main", r"tests/fidelity/components/\1")),
    (r"^components/reference-render/(.*)$", ("main", r"tests/fidelity/components/render/\1")),
    (r"^components/(harness|reference|evidence)/(.*)$", ("main", r"tests/fidelity/components/\1/\2")),
    # ---- C ABI -------------------------------------------------------------
    (r"^include/openui\.h$", ("main", "include/divui.h")),
    (r"^include/openui_style_properties\.h$", ("main", "include/divui_style_properties.h")),
    (r"^examples/c_v02/(.*)$", ("main", r"examples/c/\1")),
    # ---- tools ---------------------------------------------------------------
    (r"^tools/(setup-chromium\.sh|setup-skia\.sh|build-openui-skia\.sh|analyze_[a-z_]+\.py|gen-skia-link-deps\.py|verify-abi\.sh|install-deps\.sh)$", ("drop",)),
    (r"^tools/memory/.*$", ("drop",)),
    # Fixtures that Rust tests and Chromium captures read in place.
    (r"^tools/accountability/data/(wpt_assets|fonts)/.*$", ("main", r"\g<0>")),
    (r"^tools/accountability/data/.*$", ("gates",)),
    (r"^tools/.*$", ("main", r"\g<0>")),
    # ---- docs ------------------------------------------------------------------
    (r"^docs/progress/evidence/color-functions-v1/(.*)$", ("main", r"tests/fidelity/fixtures/color-functions-v1/\1")),
    (r"^docs/progress/evidence/.*$", ("archive",)),
    (r"^docs/progress/tracks/.*$", ("main", r"\g<0>")),
    (r"^docs/progress/.*$", ("drop",)),
    (r"^docs/renderer/generated/(" + "|".join(x.replace(".", r"\.") for x in LIVE_RENDERER_LEDGERS) + r")$", ("main", r"\g<0>")),
    (r"^docs/renderer/generated/.*$", ("gates",)),
    (r"^docs/renderer/(evidence|reproducers)/.*$", ("archive",)),
    (r"^docs/renderer/[^/]+\.md$", ("main", r"\g<0>")),
    (r"^docs/v02/evidence/.*$", ("archive",)),
    # Historical receipts beside the live v0.2 ledgers; read by no gate.
    (r"^docs/v02/generated/(native-|hosted-hardening-).*$", ("archive",)),
    (r"^docs/v02/.*$", ("main", r"\g<0>")),
    (r"^docs/alpha/.*$", ("main", r"\g<0>")),
    # audit.py reads the SP12.5 plan when it is present.
    (r"^docs/SP12\.5-PLAN\.md$", ("gates",)),
    (r"^docs/(SP[0-9.]+-[A-Z-]+\.md|SP13-R-PLAN\.md)$", ("drop",)),
    (r"^docs/(plan|analysis|learnings|api|screenshots|memory)/.*$", ("drop",)),
    (r"^docs/adr/00[123]-.*$", ("drop",)),
    (r"^docs/adr/.*$", ("main", r"\g<0>")),
    (r"^docs/architecture/(rendering-pipeline-overview|threading-model|rust-wpt-accountability)\.md$", ("main", r"\g<0>")),
    (r"^docs/architecture/.*$", ("drop",)),
    (r"^docs/(CI|DEVELOPMENT|engineering-principles)\.md$", ("main", r"\g<0>")),
]

# Repository-relative path prefixes, rewritten inside kept text files.
# Longest first; a prefix matches only at a path boundary.
PATH_PREFIXES = [
    # No trailing slash: the rewriter's lookahead needs a boundary after the
    # prefix, and `openui-*` crates fall through to their own entries.
    ("bindings/rust/openui", "crates/divui"),
    *[(f"bindings/rust/openui-{c}", f"crates/div-{c}") for c in sorted(CRATES, key=len, reverse=True)],
    ("bindings/rust/examples", "examples/rust"),
    ("bindings/rust/pixel-compare", "tests/fidelity/pixel-compare"),
    ("bindings/rust/vendor", "vendor"),
    ("bindings/rust/fuzz", "fuzz"),
    ("bindings/rust/target", "target"),
    ("bindings/rust/Cargo.toml", "Cargo.toml"),
    ("bindings/rust/Cargo.lock", "Cargo.lock"),
    ("bindings/rust/.cargo", ".cargo"),
    ("components/openui-components", "crates/div-components"),
    ("components/reference-render", "tests/fidelity/components/render"),
    ("components/harness", "tests/fidelity/components/harness"),
    ("components/reference", "tests/fidelity/components/reference"),
    ("components/evidence", "tests/fidelity/components/evidence"),
    ("components/assets", "tests/fidelity/components/assets"),
    ("components/THIRD_PARTY.md", "crates/div-components/THIRD_PARTY.md"),
    ("components/README.md", "crates/div-components/README.md"),
    ("examples/c_v02", "examples/c"),
    ("docs/progress/evidence/color-functions-v1", "tests/fidelity/fixtures/color-functions-v1"),
]

# Content is never rewritten in these (third-party, frozen or hashed bytes).
NO_REWRITE = [
    r"^vendor/",
    r"^brand/",
    r"(^|/)package-lock\.json$",
    r"^tools/accountability/data/wpt_assets/",
    r"^tools/accountability/data/fonts/sp16_linux_x86_64/",
    # Frozen v0.2 ledgers: checked only against their own pinned SHA-256.
    r"^docs/v02/generated/(baseline\.json|api-inventory\.json|migration-ledger\.csv)$",
    # Pinned SP closure inputs (test IDs only, SHA-256 pinned in scripts).
    r"^tools/wpt/sp[0-9]+[a-z]?_[a-z0-9_]+\.json$",
    r"^tools/coverage/css_grid/.*\.json$",
    r"^tools/qualification/manifests/",
    r"^tools/qualification/residual-ownership-v2\.json$",
    r"^tools/qualification/reproducers/",
    r"^tests/fidelity/components/evidence/",
    r"^tests/fidelity/components/reference/(fixtures|pages)/",
    r"^tests/fidelity/fixtures/",
    r"^crates/[^/]+/tests/[^/]*_oracles/",
    r"^crates/[^/]+/tests/fixtures/",
    r"^tests/fidelity/components/assets/",
]

# Lowercase `openui` left after the specific rules names the product in
# product code; the census tooling keeps it as its on-disk schema vocabulary
# (`openui.png`, `openui_rgba_sha256`) because frozen evidence uses it.
PRODUCT_PATHS = [
    r"^crates/", r"^examples/", r"^fuzz/", r"^\.github/", r"^tools/(release|ffi|framework|ci|style|platform|conformance|performance)/",
    r"^tools/skia_prebuilt\.py$", r"^include/", r"^site/",
]
PRODUCT_RULES = [
    (r"\bopenui_", "divui_"),
    (r"\bopenui\b", "divui"),
]

# Only crate identifiers change in the spliced census WPT ports, whose text
# and strings feed the renderer.
CRATE_IDENTS_ONLY = [
    r"^tests/fidelity/pixel-compare/src/wpt/",
]

# Tokens (regexes) that bind code to frozen evidence; never renamed anywhere.
PROTECTED = [
    r"OPENUI_ROOT_AWARE", r"OPENUI_FINAL_", r"OPENUI-BACKPORT", r"frozen-openui-",
    # Schema IDs written into committed component evidence and asset manifests.
    r"openui-components(?:-[a-z]+)*-v[0-9]+\b",
    # A sentence in the JavaScript mutation audit, whose hash the frozen
    # expanded-v35 manifest pins (written by generate_renderer_contract.py).
    r"Open UI never executes JavaScript, in any version; scripts may",
]

_IDENT_RULES = [
    (r"\bopenui-components-reference\b", "div-components-render"),
    (r"\bopenui_components_reference\b", "div_components_render"),
    (r"\bopenui-components\b", "div-components"),
    (r"\bopenui_components\b", "div_components"),
    (r"\bopenui-site-showcase\b", "div-site-showcase"),
    (r"\bopenui-fuzz\b", "div-fuzz"),
    (rf"\bopenui-({_CR})\b", r"div-\1"),
    (rf"\bopenui_({_CR.replace('-', '_')})\b", r"div_\1"),
    # The umbrella crate: package `divui`, library `div`.
    (r"\bextern crate self as openui;", "extern crate self as div;"),
    (r"(?<![\w-])openui::", "div::"),
    (r"\buse openui;", "use div;"),
]

CRATE_IDENT_RULES = _IDENT_RULES

TOKEN_RULES = _IDENT_RULES + [
    # Cargo: dependency keys, feature references, package selection.
    (r'(?m)^(\s*)openui(\s*=\s*\{|\.workspace\s*=)', r"\1divui\2"),
    (r'"openui/', '"divui/'),
    (r"\bopenui/(async|http|testing|windows|linux|desktop|menu|dialog|ffi-integration)\b", r"divui/\1"),
    (r"(--package|-p) openui\b(?![-_])", r"\1 divui"),
    (r"\bmetadata\.openui\b", "metadata.div"),
    # C ABI.
    (r"\bOPENUI_H_\b", "DIVUI_H_"),
    (r"\bOPENUI_0\.2\b", "DIVUI_0.2"),
    (r"\blibopenui\.so", "libdivui.so"),
    (r"\bopenui\.map\b", "divui.map"),
    (r"\bopenui_style_properties\.h\b", "divui_style_properties.h"),
    (r"\bopenui\.h\b", "divui.h"),
    (r"\bopenui\.pc\b", "divui.pc"),
    (r"\bopenui-sdk\b", "divui-sdk"),
    (r"\bopenui\.spdx\b", "divui.spdx"),
    (r"\bshare/openui\b", "share/divui"),
    (r"\blibopenui_(ffi)\b", r"libdiv_\1"),
    (r"\bOUI_BACKEND\b", "DIV_BACKEND"),
    (r"\bOUI_SMOKE_EXIT\b", "DIV_SMOKE_EXIT"),
    (r"\bOUI_", "DIVUI_"),
    (r"\bOui(?=[A-Z])", "DivUi"),
    (r"\boui_", "divui_"),
    (r"\bdata-oui-", "data-divui-"),
    # Environment variables.
    (r"\bOPENUI_([A-Z][A-Z0-9_]*)", r"DIV_\1"),
    # Identifiers, ids and temporary names.
    (r"\bdev\.openui\.", "build.div."),
    (r'"openui\.about"', '"div.about"'),
    (r"\bopenui-(worker|dev-stdin|render-png-hook|testing|dist|watch|uia|wayland)\b", r"div-\1"),
    (r"\bwayland-openui\b", "wayland-div"),
    (r'"openui_\{', '"div_{'),
    # The CLI.
    (r"\bopenui (new|dev|build|doctor)\b", r"div \1"),
    (r'"openui: ', '"div: '),
    # Prose and URLs.
    (r"\b(an|An) Open UI\b", lambda m: ("a" if m.group(1) == "an" else "A") + " Div"),
    (r"\bOpen UI\b", "Div"),
    # CamelCase in code (CMake packages and targets); Markdown prose says "Div".
    (r"\bOpenUI", "DivUi"),
    (r"https://github\.com/zhuowcui/open-ui(\.git)?(?=$|[\s\"'`)>\]#,;])", r"https://github.com/build-with-div/div\1"),
]
