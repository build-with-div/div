"""Targeted edits that token rules cannot express.

Each entry is (new path, regex, replacement, expected match count). The
script fails when a count differs, so a change in the source surfaces here
instead of producing a silently different tree. Applied after token rules.
"""

FIXUPS = [
    # ---- one workspace at the repository root ------------------------------
    ("Cargo.toml", r'homepage = "https://github.com/build-with-div/div"',
     'homepage = "https://div.build"', 1),
    ("Cargo.toml", r'keywords = \["ui", "desktop", "skia", "linux"\]\n',
     'keywords = ["ui", "desktop", "skia", "linux"]\n\n'
     '# Shared by the components crate and its reference renderer.\n'
     '[workspace.dependencies]\n'
     'divui = { version = "=0.2.0", path = "crates/divui" }\n'
     'div-components = { path = "crates/div-components" }\n'
     'serde_json = "1.0"\n', 1),
    # ---- the umbrella crate: package `divui`, library `div` -----------------
    # (Product rules already renamed the package to `divui`.)
    ("crates/divui/Cargo.toml", r"\n\[features\]\n",
     '\n[lib]\n# Applications write `use div::prelude::*`.\nname = "div"\n\n[features]\n', 1),
    # ---- the CLI binary is `div` --------------------------------------------
    ("crates/div-cli/Cargo.toml", r'\[\[bin\]\]\nname = "divui"', '[[bin]]\nname = "div"', 1),
    # The CLI finds the framework at a checkout's root, in crates/divui.
    ("crates/div-cli/src/new.rs",
     r"/// Accept a checkout's root or its `bindings/rust` directory and return the\n/// latter, absolute\.",
     "/// Accept a Div checkout's root and return it, absolute.", 1),
    ("crates/div-cli/src/new.rs", r'let candidates = \[path\.join\("bindings/rust"\), path\.to_path_buf\(\)\];',
     "let candidates = [path.to_path_buf()];", 1),
    ("crates/div-cli/src/new.rs", r'candidate\.join\("divui/Cargo\.toml"\)',
     'candidate.join("crates/divui/Cargo.toml")', 1),
    ("crates/div-cli/src/template.rs", r'self\.locate\("divui"\)', 'self.locate("crates/divui")', 1),
    ("crates/div-cli/src/template.rs", r"/// The `bindings/rust` directory of a local checkout\.",
     "/// The root of a local Div checkout.", 1),
    ("crates/div-cli/src/template.rs", r"/// A TOML inline table locating `relative` \(a path inside\n    /// `bindings/rust`\) in this source\.",
     "/// A TOML inline table locating `relative` (a path inside the\n    /// checkout) in this source.", 1),
    # ---- tests that walk up from a crate to the repository root -------------
    ("crates/div-paint/tests/pixel_comparison_tests.rs",
     r"        \.parent\(\) // bindings/rust\n        \.unwrap\(\)\n        \.parent\(\) // bindings\n",
     "        .parent() // crates\n", 1),
    ("crates/div-paint/tests/pixel_comparison_tests.rs", r"We're in crates/div-paint/, so go up 3 levels\.",
     "We're in crates/div-paint/, so go up 2 levels.", 1),
    # ---- workflows: the workspace is the repository root --------------------
    *[(f".github/workflows/{w}.yml", r"\n *working-directory: bindings/rust(?=\n)", "", -1)
      for w in ("ci", "hardening", "release")],
    *[(f".github/workflows/{w}.yml", r"'bindings/rust/\*\*/Cargo\.toml'", "'**/Cargo.toml'", -1)
      for w in ("ci", "hardening")],
    (".github/workflows/ci.yml", r"--manifest-path Cargo\.toml ", "", -1),
    # Gates that read evidence restore the pinned data repository first.
    (".github/workflows/ci.yml", r"(\n      - name: Verify the Windows-pending test ledger\n)",
     "\n      - name: Restore the pinned fidelity data\n        run: python tools/fidelity_data.py fetch\n\\1", 1),
    (".github/workflows/ci.yml",
     r"(  historical-pixel-audit:\n(?:.*\n)*?      - uses: actions/checkout@v6\n)",
     "\\1\n      - name: Restore the pinned fidelity data\n        run: python3 tools/fidelity_data.py fetch\n", 1),
    (".github/workflows/ci.yml", r"(\n      - name: Run closure, porter, and renderer evidence tests\n)",
     "\n      - name: Restore the pinned fidelity data\n        run: python3 tools/fidelity_data.py fetch\n\\1", 1),
    # Components are workspace members now.
    (".github/workflows/components.yml", r'      - "components/\*\*"\n      - "bindings/rust/\*\*"\n',
     '      - "crates/**"\n      - "tests/fidelity/components/**"\n      - "Cargo.lock"\n', 2),
    (".github/workflows/components.yml", r"# The components track \(components/\)", "# The components track (crates/div-components, tests/fidelity/components)", 1),
    (".github/workflows/components.yml", r"the components workspace over\n# the public `divui` crate",
     "the components crates over\n# the public `divui` crate", 1),
    (".github/workflows/components.yml", r"            components/target\n", "            target\n", 1),
    (".github/workflows/components.yml",
     r"hashFiles\('components/Cargo\.lock', 'components/\*\*/Cargo\.toml', 'bindings/rust/\*\*/Cargo\.toml'\)",
     "hashFiles('Cargo.lock', '**/Cargo.toml')", 1),
    (".github/workflows/components.yml", r"        working-directory: components\n        run: cargo fmt --all --check",
     "        run: cargo fmt --all --check", 1),
    (".github/workflows/components.yml",
     r"      - name: Test the components workspace\n        working-directory: components\n        run: cargo test --locked --no-fail-fast",
     "      - name: Test the components crates\n        run: cargo test --locked --no-fail-fast --package div-components --package div-components-render", 1),
    # No C++ sources or GN files remain; only the C ABI consumers are formatted.
    (".github/workflows/format-check.yml", r"find src include examples -type f", "find include examples/c -type f", 1),
    (".github/workflows/format-check.yml", r"\n  gn-format:\n(?:.*\n)*$", "\n", 1),
    # Use the clang-format the other jobs and the import use (pip 18.1.8);
    # Ubuntu's clang-format-18 is 18.1.3, which formats some macros differently.
    (".github/workflows/format-check.yml",
     r"      - name: Install clang-format\n        run: \|\n          sudo apt-get update\n"
     r"          sudo apt-get install --yes --no-install-recommends clang-format-18\n",
     "      - uses: actions/setup-python@v5\n        with:\n          python-version: \"3.12\"\n\n"
     "      - name: Install clang-format\n        run: python3 -m pip install clang-format==18.1.8\n", 1),
    (".github/workflows/format-check.yml", r"xargs -0 --no-run-if-empty clang-format-18 --dry-run --Werror",
     "xargs -0 --no-run-if-empty clang-format --dry-run --Werror", 1),
    # ---- Python paths built from segments ---------------------------------
    ("tools/accountability/audit.py", r'"bindings", "rust", "pixel-compare"', '"tests", "fidelity", "pixel-compare"', 1),
    *[(f"tools/accountability/{t}.py", r'"bindings", "rust",\s*', "", 1)
      for t in ("report", "run_all_pixel_comparisons", "update_tracking")],
    ("tools/framework/check_cli_template.py", r'WORKSPACE = REPO / "bindings" / "rust"', "WORKSPACE = REPO", 1),
    ("tools/skia_prebuilt.py", r'WORKSPACE = REPO / "bindings" / "rust"', "WORKSPACE = REPO", 1),
    ("tests/fidelity/components/harness/reference_harness.py",
     r"ROOT = Path\(__file__\)\.resolve\(\)\.parents\[2\]", "ROOT = Path(__file__).resolve().parents[4]", 1),
    # ---- components join the workspace but stay unpublished at 0.0.0 --------
    ("crates/div-components/Cargo.toml", r"(?m)^version\.workspace = true", 'version = "0.0.0"', 1),
    ("crates/div-components/Cargo.toml", r"publish\.workspace = true", "publish = false", 1),
    ("tests/fidelity/components/render/Cargo.toml", r"(?m)^version\.workspace = true", 'version = "0.0.0"', 1),
    ("tests/fidelity/components/render/Cargo.toml", r"publish\.workspace = true", "publish = false", 1),
    # ---- tools that named the old workspace root `bindings/rust` -------------
    ("tools/qualification/generate_renderer_contract.py", r'\(ROOT / "bindings/rust" / area\)',
     '(ROOT / "crates" / area)', 1),
    ("tools/ci/check_platform_scoped_tests.py", r'SOURCES = ROOT / "bindings/rust"', "SOURCES = ROOT", 1),
    ("tools/ci/check_platform_scoped_tests.py",
     r'        if "vendor" in parts or "target" in parts:\n',
     '        if "vendor" in parts or "target" in parts or parts[0] in ("out", ".git"):\n', 1),
    ("tools/qualification/build_renderer.py", r'cwd=root / "bindings/rust", ', "cwd=root, ", 1),
    ("tools/qualification/native_collapsed_tables/run_matrix.py", r"cwd=ROOT/'bindings/rust'\)", "cwd=ROOT)", 1),
    ("tools/release/build_v02_linux.py", r'RUST = ROOT / "bindings/rust"', "RUST = ROOT", 1),
    ("tools/release/build_v02_linux.py", r'RUST / "examples" / example / "src"',
     'RUST / "examples" / "rust" / example / "src"', 1),
    ("tools/release/build_v02_linux.py", r'RUST / "div-text/fonts"', 'RUST / "crates/div-text/fonts"', 1),
    ("tools/release/build_v02_windows.py", r'RUST / "examples" / example / "src"',
     'RUST / "examples" / "rust" / example / "src"', 1),
    ("tools/release/build_v02_windows.py", r'RUST / "div-text/fonts"', 'RUST / "crates/div-text/fonts"', 1),
    # Cargo metadata is `[workspace.metadata.div]` and `[package.metadata.div]`.
    ("tools/release/build_v02_linux.py", r'\.get\(\n        "divui", \{\}\n',
     '.get(\n        "div", {}\n', 1),
    ("crates/div-cli/src/cargo.rs", r'&package\["metadata"\]\["divui"\]', '&package["metadata"]["div"]', 1),
    ("crates/div-cli/src/cargo.rs", r'"metadata": \{ "divui": \{', '"metadata": { "div": {', 1),
    # The API inventory covers the published crates, as `openui*` did.
    ("tools/release/generate_v02_contract.py",
     r'    for path in sorted\(\(ROOT / "bindings/rust"\)\.glob\("divui\*/src/\*\*/\*\.rs"\)\):\n',
     '    for path in sorted((ROOT / "crates").glob("div*/src/**/*.rs")):\n'
     '        if path.relative_to(ROOT / "crates").parts[0] == "div-components":\n'
     '            continue  # unpublished, outside the v0.2 API\n', 1),
    ("tools/style/migrate_computed_style_diagnostics.py", r'RUST = ROOT / "bindings/rust"', "RUST = ROOT", 1),
    ("tools/style/migrate_native_style_tests.py", r'RUST = ROOT / "bindings/rust"', 'RUST = ROOT / "crates"', 1),
    ("tools/wpt/splice_text_port.py", r'PROJECT_ROOT, "bindings", "rust", "pixel-compare", "src", "wpt"',
     'PROJECT_ROOT, "tests", "fidelity", "pixel-compare", "src", "wpt"', 1),
    ("tools/wpt/splice_text_port.py", r'cwd=os\.path\.join\(PROJECT_ROOT, "bindings", "rust"\),',
     "cwd=PROJECT_ROOT,", 1),
    ("tools/accountability/run_all_pixel_comparisons.py", r"Build with: cd bindings/rust && cargo build",
     "Build with: cargo build", 1),
    ("crates/div-cli/src/template.rs", r"/src/open-ui/bindings/rust", "/src/div", 2),
    ("crates/div-cli/src/template.rs", r'r"\\\\\?\\E:\\open-ui\\bindings\\rust"', r'r"\\\\?\\E:\\div"', 1),
    ("crates/div-cli/src/template.rs", r'"E:/open-ui/bindings/rust"', '"E:/div"', 1),
    *[(f"crates/div-paint/tests/{t}.rs", r"//! cd bindings/rust\n", "", 1)
      for t in ("pixel_comparison_tests", "sp12_i1_block_pixel_tests", "sp12_i2_float_pixel_tests",
                "sp12_i3_position_pixel_tests", "sp13_j_inline_pixel_tests")],
    # ---- the components README describes the new layout ----------------------
    ("crates/div-components/README.md",
     r"\| `Cargo\.toml` \| A separate Cargo workspace over `crates/divui`, repeating that workspace's patches\. \|\n",
     "", 1),
    *[("crates/div-components/README.md", rf"(?m)^\| `{re_old}` \|", f"| `{new}` |", 1)
      for re_old, new in (
          (r"div-components/", "crates/div-components/"),
          (r"assets/", "tests/fidelity/components/assets/"),
          (r"reference-render/", "tests/fidelity/components/render/"),
          (r"reference/", "tests/fidelity/components/reference/"),
          (r"reference/pages/", "tests/fidelity/components/reference/pages/"),
          (r"harness/", "tests/fidelity/components/harness/"),
          (r"evidence/", "tests/fidelity/components/evidence/"),
          (r"THIRD_PARTY\.md", "crates/div-components/THIRD_PARTY.md"),
      )],
    ("crates/div-components/README.md", r"--manifest-path components/Cargo\.toml ", "", 1),
    ("crates/div-components/README.md", r"(?<![/\w])`div-components/src/", "`crates/div-components/src/", -1),
    # build.mjs writes these strings into the committed (frozen) asset manifest
    # and reference pages; keep their bytes, so `build.mjs --check` holds.
    ("tests/fidelity/components/reference/build.mjs",
     r'"\\n/\* Bundled fonts \(tests/fidelity/components/reference/build\.mjs\)\. \*/\\n"',
     r'"\\n/* Bundled fonts (components/reference/build.mjs). */\\n"', 1),
    ("tests/fidelity/components/reference/build.mjs",
     r'note: "Generated by tests/fidelity/components/reference/build\.mjs; files are unmodified copies '
     r'from the npm packages pinned by tests/fidelity/components/reference/package-lock\.json\.",',
     'note: "Generated by components/reference/build.mjs; files are unmodified copies '
     'from the npm packages pinned by components/reference/package-lock.json.",', 1),
    # The CLI's own tests: its binary is `div` and the checkout root is the workspace.
    ("crates/div-cli/src/cargo.rs",
     r'        assert_eq!\(app\.bins, \["divui"\]\);\n        assert_eq!\(app\.bin\(None\)\.unwrap\(\), "divui"\);\n',
     '        assert_eq!(app.bins, ["div"]);\n        assert_eq!(app.bin(None).unwrap(), "div");\n', 1),
    ("crates/div-cli/src/new.rs",
     r"    fn framework_path_accepts_the_checkout_root_or_its_workspace\(\) \{\n(?:.*\n)*?    \}\n",
     "    fn framework_path_accepts_the_checkout_root() {\n"
     '        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();\n'
     "        let expected = cargo::canonical(root).unwrap();\n"
     "        assert_eq!(framework_workspace(root).unwrap(), expected);\n"
     '        assert!(framework_workspace(&root.join("crates")).is_err());\n'
     "    }\n", 1),
    # The CLI binary is `div`.
    ("tools/framework/check_cli_template.py", r'cli_target / "debug" / f"divui\{EXE\}"',
     'cli_target / "debug" / f"div{EXE}"', 1),
    # The workspace lockfile now sits at the fixture root, which already exists.
    ("tools/release/test_dependency_security.py",
     r"            path = self\.root / name\n            path\.parent\.mkdir\(parents=True\)\n",
     "            path = self.root / name\n            path.parent.mkdir(parents=True, exist_ok=True)\n", 1),
    # ---- docs: the workspace is the repository root; legacy code stays behind -
    ("CONTRIBUTING.md",
     r"Div v0\.2 is developed in the pure-Rust workspace under `bindings/rust`\.\n"
     r"The root GN/C\+\+ Blink backend and the SP2 experiments are historical evidence;\n"
     r"do not add new product features to them\.",
     "Div v0.2 is developed in the pure-Rust Cargo workspace at the repository root.\n"
     "The GN/C++ Blink backend and the SP2 experiments stay in the archived\n"
     "[Open UI repository](https://github.com/zhuowcui/open-ui); they are not part\n"
     "of Div.", 1),
    ("docs/DEVELOPMENT.md",
     r"The supported workspace is `bindings/rust`\. It contains the one pure-Rust\nengine",
     "The supported workspace is the Cargo workspace at the repository root. It\n"
     "contains the one pure-Rust engine", 1),
    ("docs/DEVELOPMENT.md", r"cargo build --manifest-path Cargo\.toml --locked -p pixel-compare",
     "cargo build --locked -p pixel-compare", 1),
    ("docs/DEVELOPMENT.md",
     r"`\.cargo/config\.chromium\.toml` records the maintainer workstation's\n(?:.*\n)*?"
     r"Never make this file the default or use it for release artifacts\. Hosted CI\n"
     r"sets an equivalent portable toolchain explicitly\.\n",
     "The frozen Chromium-compatible raster environment was reproduced with a\n"
     "Cargo profile of the maintainer workstation's pinned Chromium sysroot and\n"
     "compiler paths, kept in the archived\n"
     "[Open UI repository](https://github.com/zhuowcui/open-ui). Hosted CI sets an\n"
     "equivalent portable toolchain explicitly.\n", 1),
    ("docs/DEVELOPMENT.md", r"\.\.\\\.\.\\tools\\", r"tools\\", -1),
    ("docs/DEVELOPMENT.md",
     r"The root GN/C\+\+ Blink backend, `openui-build`,\n`openui-sys`, and SP2 Skia experiments are historical evidence only\. Do not add\n"
     r"new application features to them or include them in the Rust workspace or\nrelease packages\.",
     "The GN/C++ Blink backend, `openui-build`,\n`openui-sys`, and the SP2 Skia experiments stay in the archived Open UI\n"
     "repository; they are not part of Div.", 1),
    ("docs/progress/tracks/README.md",
     r"conflict\. `docs/progress/current-status\.md` holds the history before the\nalpha tracks and is no longer appended to\.",
     "conflict. The history before the alpha tracks stays in the archived Open UI\nrepository.", 1),
    # ---- the lockfile names the umbrella package; cargo re-sorts it later ----
    ("Cargo.lock", r'(?m)^name = "openui"$', 'name = "divui"', 1),
    ("Cargo.lock", r'(?m)^ "openui",$', ' "divui",', -1),
]
