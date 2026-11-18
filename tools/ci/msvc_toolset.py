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
- Skia's GN build ignores both and takes the highest toolset it finds, so
  `tools/skia_prebuilt.py`, which runs every CI cargo command that compiles
  Skia, passes `VCToolsInstallDir`'s version as GN's `win_toolchain_version`.

    python tools/ci/msvc_toolset.py list
    python tools/ci/msvc_toolset.py verify

`list` prints every Visual Studio instance and its toolsets. `verify` fails
unless the pinned toolset exists, the pins agree, and clang compiles a
C++ translation unit against exactly that toolset's STL. It never relaxes
the STL's compiler check (no `_ALLOW_COMPILER_AND_STL_VERSION_MISMATCH`).
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


def cmd_verify(_: argparse.Namespace) -> None:
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
    skia_version = gn_arg(skia_prebuilt.pin_msvc_toolset(dict(os.environ)).get("SKIA_GN_ARGS", ""), "win_toolchain_version")
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
    sub.add_parser("verify", help="check the pinned toolset exists and clang uses it")
    args = parser.parse_args(argv)
    {"list": cmd_list, "verify": cmd_verify}[args.command](args)


if __name__ == "__main__":
    main()
