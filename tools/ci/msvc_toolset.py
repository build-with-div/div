#!/usr/bin/env python3
"""List and verify the MSVC toolset a Windows job compiles against.

A hosted runner image can add a newer MSVC toolset at any time, and every
tool that picks "the newest" then changes compiler inputs under us. On
`windows-2025-vs2026` 20261116.315.1, VS 18's default toolset 14.52.37025
ships an STL that requires Clang 22 (yvals_core.h STL1000), while the image's
LLVM is Clang 20. CI therefore pins one toolset, by version, for every
consumer, in the job's `env`:

- `VCToolsVersion`: cc-rs and rustc's linker lookup (find-msvc-tools) search
  only this toolset in each Visual Studio instance.
- `VCToolsInstallDir`: clang and libclang (bindgen) take the STL and the
  MSVC libraries from this directory instead of the newest Visual Studio.
- Skia's GN build ignores both and takes the highest toolset in the VC
  directory rust-skia resolves. Where cargo runs through
  `tools/skia_prebuilt.py` (windows-msvc), that script passes
  `VCToolsInstallDir`'s version as GN's `win_toolchain_version`. A plain
  cargo build of a `div new` app (cli-template) takes Skia's options from
  the app's own config, so `skia-vc` instead gives rust-skia a VC directory
  that holds only the pinned toolset.

    python tools/ci/msvc_toolset.py list
    python tools/ci/msvc_toolset.py skia-vc              # plain cargo builds only
    python tools/ci/msvc_toolset.py verify [--skia prebuilt|app]

`list` prints every Visual Studio instance and its toolsets. `verify` fails
unless the pinned toolset exists, the pins agree, Skia would compile against
the same toolset, and clang compiles a C++ translation unit against exactly
that toolset's STL. It never relaxes the STL's compiler check (no
`_ALLOW_COMPILER_AND_STL_VERSION_MISMATCH`).
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools import skia_prebuilt  # noqa: E402

PROBE = "#include <cstdio>\n#include <vector>\nint main() { std::vector<int> v{1}; std::printf(\"%d\", v[0]); }\n"


def vswhere() -> Path:
    root = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"))
    return root / "Microsoft Visual Studio" / "Installer" / "vswhere.exe"


def instances() -> list[dict]:
    tool = vswhere()
    if not tool.exists():
        raise SystemExit(f"msvc_toolset: {tool} not found")
    result = subprocess.run(
        [str(tool), "-all", "-products", "*", "-prerelease", "-format", "json", "-utf8"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    return json.loads(result.stdout)


def toolsets(installation: Path) -> list[str]:
    tools = installation / "VC" / "Tools" / "MSVC"
    if not tools.is_dir():
        return []
    return sorted(p.name for p in tools.iterdir() if (p / "include" / "yvals_core.h").exists())


def cmd_list(_: argparse.Namespace) -> None:
    for instance in instances():
        path = Path(instance["installationPath"])
        print(f"{instance.get('displayName', '?')} {instance.get('installationVersion', '?')}: {path}")
        for version in toolsets(path):
            print(f"  {version}")
    clang = shutil.which("clang++")
    if clang:
        print(subprocess.run([clang, "--version"], capture_output=True, text=True).stdout.splitlines()[0])


def gn_arg(gn_args: str, name: str) -> str | None:
    match = re.search(rf'\b{name}\s*=\s*"([^"]*)"', gn_args)
    return match.group(1) if match else None


def same_path(a: str | Path, b: str | Path) -> bool:
    return os.path.normcase(os.path.normpath(str(a))) == os.path.normcase(os.path.normpath(str(b)))


def msvc_includes(clang_verbose: str) -> list[str]:
    """The MSVC STL directories in clang -v's `#include <...>` search list."""
    lines = clang_verbose.splitlines()
    try:
        start = lines.index("#include <...> search starts here:") + 1
        end = lines.index("End of search list.", start)
    except ValueError:
        return []
    return [
        line.strip()
        for line in lines[start:end]
        if re.search(r"[\\/]VC[\\/]Tools[\\/]MSVC[\\/]", line, re.IGNORECASE)
    ]


# rust-skia's resolve_vc() (skia-bindings/build_support/platform/windows.rs):
# VCINSTALLDIR, else the first of these that exists. Without one, Skia's own
# gn/find_msvc.py searches every Visual Studio and takes the first it finds.
RUST_SKIA_VC_DIRS = [
    rf"C:\{program_files}\Microsoft Visual Studio\{release}\{edition}\VC"
    for program_files, release in (("Program Files", "2022"), ("Program Files (x86)", "2019"))
    for edition in ("BuildTools", "Enterprise", "Professional", "Community")
]
# The view `skia-vc` creates: the first directory rust-skia tries.
SKIA_VC_VIEW = Path(RUST_SKIA_VC_DIRS[0])
TOOLSET_NAME = re.compile(r"[0-9]{2}\.[0-9]{2}\.[0-9]{5}")


def rust_skia_vc(env: dict[str, str], candidates: list[str] = RUST_SKIA_VC_DIRS) -> Path | None:
    install_dir = skia_prebuilt.env_value(env, "VCINSTALLDIR")
    if install_dir:
        return Path(install_dir.rstrip("\\"))
    return next((Path(d) for d in candidates if Path(d).exists()), None)


def highest_toolset(vc: Path) -> str | None:
    """Skia's gn/highest_version_dir.py over ``<vc>/Tools/MSVC``."""
    tools = vc / "Tools" / "MSVC"
    names = sorted(p.name for p in tools.iterdir() if TOOLSET_NAME.match(p.name)) if tools.is_dir() else []
    return names[-1] if names else None


def skia_toolset(env: dict[str, str], mode: str, candidates: list[str] = RUST_SKIA_VC_DIRS) -> str | None:
    """The MSVC toolset rust-skia's Skia build would compile against.

    ``prebuilt``: through tools/skia_prebuilt.py, which pins
    ``win_toolchain_version`` in SKIA_GN_ARGS. ``app``: a plain cargo build
    (an app made by `div new`), where Skia takes the highest toolset in the
    VC directory rust-skia resolves.
    """
    if mode == "prebuilt":
        return gn_arg(skia_prebuilt.pin_msvc_toolset(env).get("SKIA_GN_ARGS", ""), "win_toolchain_version")
    pinned = gn_arg(skia_prebuilt.env_value(env, "SKIA_GN_ARGS"), "win_toolchain_version")
    if pinned:
        return pinned
    vc = rust_skia_vc(env, candidates)
    return highest_toolset(vc) if vc else None


def cmd_skia_vc(_: argparse.Namespace) -> None:
    """Give a plain cargo build's Skia a VC directory with only the pinned toolset.

    rust-skia's first fallback directory becomes a VC directory whose
    Tools/MSVC holds one entry, a junction to the pinned toolset. Skia's GN
    reads only ``Tools/MSVC/<version>/{include,lib,bin}`` there. Visual
    Studio does not register it as an instance, so cc-rs, rustc and clang
    still find their tools through VCToolsVersion and VCToolsInstallDir.
    """
    env = dict(os.environ)
    install_dir = Path(skia_prebuilt.env_value(env, "VCToolsInstallDir").rstrip("\\/"))
    version = install_dir.name
    if not (install_dir / "include" / "yvals_core.h").exists():
        raise SystemExit(f"msvc_toolset: VCToolsInstallDir {install_dir} has no STL")
    existing = rust_skia_vc(env)
    if existing is not None:
        raise SystemExit(f"msvc_toolset: rust-skia already resolves {existing}; not adding a view")
    link = SKIA_VC_VIEW / "Tools" / "MSVC" / version
    link.parent.mkdir(parents=True)
    subprocess.run(["cmd", "/c", "mklink", "/J", str(link), str(install_dir)], check=True)
    print(f"msvc_toolset: Skia's VC directory {SKIA_VC_VIEW} holds only {version}")


def cmd_verify(args: argparse.Namespace) -> None:
    version = skia_prebuilt.env_value(dict(os.environ), "VCToolsVersion")
    install_dir = skia_prebuilt.env_value(dict(os.environ), "VCToolsInstallDir")
    errors = []
    if not version or not install_dir:
        raise SystemExit("msvc_toolset: VCToolsVersion and VCToolsInstallDir must be set")
    tools = Path(install_dir)
    if not (tools / "include" / "yvals_core.h").exists():
        errors.append(f"VCToolsInstallDir {tools} has no STL; this image does not ship the pinned toolset")
    if tools.name != version and Path(install_dir.rstrip("\\/")).name != version:
        errors.append(f"VCToolsInstallDir {tools} is not toolset {version}")
    if not (tools / "bin" / "Hostx64" / "x64" / "link.exe").exists():
        errors.append(f"{tools} has no x64 link.exe")
    skia_version = skia_toolset(dict(os.environ), args.skia)
    if skia_version != version:
        errors.append(f"Skia would compile against MSVC {skia_version!r}, not {version}")
    if errors:
        for error in errors:
            print(f"msvc_toolset: {error}", file=sys.stderr)
        print("msvc_toolset: installed toolsets:", file=sys.stderr)
        cmd_list(argparse.Namespace())
        raise SystemExit(1)

    clang = shutil.which("clang++")
    if not clang:
        raise SystemExit("msvc_toolset: clang++ not found")
    with tempfile.TemporaryDirectory() as tmp:
        source = Path(tmp) / "probe.cc"
        source.write_text(PROBE, encoding="utf-8")
        result = subprocess.run(
            [clang, "-v", "-fsyntax-only", str(source)], capture_output=True, text=True
        )
    output = result.stdout + result.stderr
    if result.returncode != 0:
        print(output, file=sys.stderr)
        raise SystemExit("msvc_toolset: clang++ rejected the pinned toolset's STL")
    includes = msvc_includes(output)
    if not includes or not all(same_path(Path(i), tools / "include") for i in includes):
        print(output, file=sys.stderr)
        raise SystemExit(f"msvc_toolset: clang++ used {includes}, not {tools / 'include'}")
    clang_version = output.splitlines()[0] if output else "?"
    print(f"msvc_toolset: {clang_version} compiles against MSVC {version} ({tools})")


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("list", help="print Visual Studio instances and their MSVC toolsets")
    verify = sub.add_parser("verify", help="check the pinned toolset exists and clang uses it")
    verify.add_argument(
        "--skia",
        choices=("prebuilt", "app"),
        default="prebuilt",
        help="how the job compiles Skia: through tools/skia_prebuilt.py, or a plain cargo build",
    )
    sub.add_parser("skia-vc", help="give a plain cargo build's Skia only the pinned toolset")
    args = parser.parse_args(argv)
    {"list": cmd_list, "verify": cmd_verify, "skia-vc": cmd_skia_vc}[args.command](args)


if __name__ == "__main__":
    main()
