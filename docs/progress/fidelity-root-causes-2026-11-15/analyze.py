#!/usr/bin/env python3
"""Reproduce the fidelity root-cause report's numbers from a census run.

Input is the per-row output of an expanded renderer census
(`run_renderer_matrix.py --suite expanded`): its `expanded-summary.json`
and the `<profile>/<id>/{chromium,openui}.png` pairs next to it. The script

1. lists every non-exact row with its font profile, raster configuration,
   mismatched pixels and largest channel delta;
2. measures pixel features of each row (whether mismatches sit on colour
   edges, whether they form areas or one-pixel lines, whether each side is a
   flat fill colour or a blend);
3. puts each test in one stratum (real-font text, area differences, thin
   edges failing only at fractional scales, other thin edges) by its largest
   row, and draws the seeded random sample that was reviewed by eye;
4. applies the reviewed labels (LABELS below) and scales them to every test
   and row of each stratum.

Only steps 1 and 2 measure every row. Step 4 is an estimate from the
reviewed sample; the report keeps the two apart.

    python3 analyze.py --run <results dir> --output root-causes.json
"""

from __future__ import annotations

import argparse
import collections
import json
import math
import random
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np
from PIL import Image

SCALES = {
    "legacy-800x600@1": "1",
    "mobile-375x667@2": "2",
    "desktop-1280x720@1.25": "1.25",
    "desktop-1920x1080@1.5": "1.5",
}
FRACTIONAL = {"1.25", "1.5"}
SEED = 2026
SAMPLE_SIZES = {"fractional-thin": 30, "other-thin": 30, "real-font": 15, "area": 15}

# Reviewed root cause of each sampled test, by sample index (see the report):
# F sub-pixel geometry of axis-aligned edges, T text glyphs,
# I image and background-image tiles, A diagonal and curved edges,
# C native controls and scrollbars, L layout, P clip and effect geometry,
# B dotted and dashed borders.
LABELS = {
    **dict.fromkeys([1, 2, 3, 4, 6, 7, 8, 10, 11, 12, 13, 14, 16, 17, 18, 19, 20,
                     21, 22, 23, 24, 25, 26, 28, 29, 35, 36, 37, 39, 47, 56,
                     62, 64, 66, 70], "F"),
    **dict.fromkeys([0, 5, 9, 15, 32, 33, 38, 40, 43, 51, 54, 57], "I"),
    **dict.fromkeys([27, 30, 41, 42, 46, 48, 53, 87], "A"),
    **dict.fromkeys([31, 34, 45, 49, 50, 52, 58, 83, 84], "C"),
    **dict.fromkeys([44, 59, 61, 65, 67, 69, 71, 72, 73, 74], "T"),
    **dict.fromkeys([55, 60, 63, 68, 77, 78, 80, 81, 85, 86, 88, 89], "L"),
    **dict.fromkeys([75, 79], "P"),
    **dict.fromkeys([76, 82], "B"),
}


def load(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.int32)


def features(job: tuple[str, str, str]) -> dict:
    run, profile, test_id = job
    d = Path(run) / profile / test_id
    c, o = load(d / "chromium.png"), load(d / "openui.png")
    pc = (c[..., 0] << 16) | (c[..., 1] << 8) | c[..., 2]
    po = (o[..., 0] << 16) | (o[..., 1] << 8) | o[..., 2]
    mask = pc != po
    n = int(mask.sum())
    h, w = mask.shape

    def edges(p):
        pad = np.pad(p, 1, mode="edge")
        e = np.zeros_like(mask)
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                if dx or dy:
                    e |= pad[1 + dy:1 + dy + h, 1 + dx:1 + dx + w] != p
        return e

    min_count = max(64, int(0.0005 * h * w))
    flat = set()
    for p in (pc, po):
        vals, counts = np.unique(p, return_counts=True)
        flat |= set(vals[counts >= min_count].tolist())
    c_flat = np.isin(pc[mask], list(flat))
    o_flat = np.isin(po[mask], list(flat))
    pad = np.pad(mask, 1)
    eroded = np.ones_like(mask)
    for dy in (-1, 0, 1):
        for dx in (-1, 0, 1):
            eroded &= pad[1 + dy:1 + dy + h, 1 + dx:1 + dx + w]
    return {
        "id": test_id,
        "profile": profile,
        "n": n,
        "area_fraction": n / (h * w),
        "on_edges": float((mask & (edges(pc) | edges(po))).sum() / n),
        "in_areas": float(eroded.sum() / n),
        "both_flat": float((c_flat & o_flat).sum() / n),
        "chromium_blend_div_flat": float((~c_flat & o_flat).sum() / n),
        "div_blend_chromium_flat": float((c_flat & ~o_flat).sum() / n),
        "both_blend": float((~c_flat & ~o_flat).sum() / n),
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--run", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--jobs", type=int, default=16)
    args = parser.parse_args(argv)

    summary = json.loads((args.run / "expanded-summary.json").read_text(encoding="utf-8"))
    rows = []
    for profile in summary["profiles"]:
        for test in profile["tests"]:
            if test["status"] == "exact":
                continue
            deltas = test["diff_signature"]["channel_deltas"]
            rows.append({
                "id": test["id"],
                "profile": profile["profile"],
                "status": test["status"],
                "font_profile": test["font_profile"],
                "raster_configuration": test["raster_configuration"],
                "mismatched_pixels": test["mismatched_pixels"],
                "max_channel_delta": max(v["maximum_absolute_delta"] for v in deltas.values()),
            })
    # Rows stay in the summary's order (profile by profile): a test's largest
    # row, which picks its stratum, is the first one in that order on a tie.
    with ProcessPoolExecutor(args.jobs) as pool:
        feats = list(pool.map(features, [(str(args.run), r["profile"], r["id"]) for r in rows],
                              chunksize=8))
    for row, feat in zip(rows, feats):
        assert (row["id"], row["profile"]) == (feat["id"], feat["profile"])
        assert row["mismatched_pixels"] == feat["n"], row
        row.update({k: round(v, 6) for k, v in feat.items() if k not in ("id", "profile", "n")})

    scales = collections.defaultdict(set)
    for row in rows:
        scales[row["id"]].add(SCALES[row["profile"]])

    def first_pass(row):
        if row["font_profile"] == "registered-real-font":
            return "real-font"
        if row["in_areas"] > 0.3 or row["area_fraction"] > 0.002:
            return "area"
        if scales[row["id"]] <= FRACTIONAL:
            return "fractional-thin"
        return "other-thin"

    largest = {}
    for row in rows:
        row["first_pass"] = first_pass(row)
        best = largest.get(row["id"])
        if best is None or row["mismatched_pixels"] > best["mismatched_pixels"]:
            largest[row["id"]] = row
    stratum = {test_id: row["first_pass"] for test_id, row in largest.items()}
    for row in rows:
        row["stratum"] = stratum[row["id"]]
        row["failing_scales"] = sorted(scales[row["id"]], key=float)

    rng = random.Random(SEED)
    sample = []
    for name, size in SAMPLE_SIZES.items():
        tests = sorted((test_id, largest[test_id]["profile"]) for test_id, s in stratum.items()
                       if s == name)
        rng.shuffle(tests)
        sample += [(name, test_id, profile) for test_id, profile in tests[:size]]
    assert sorted(LABELS) == list(range(len(sample)))
    reviewed = {test_id: LABELS[i] for i, (_, test_id, _) in enumerate(sample)}
    for row in rows:
        row["reviewed_cause"] = reviewed.get(row["id"])

    tests_in = collections.Counter(stratum.values())
    rows_in = collections.Counter(row["stratum"] for row in rows)
    rows_of = collections.Counter(row["id"] for row in rows)
    estimate = collections.defaultdict(lambda: {"tests": 0.0, "tests_var": 0.0, "rows": 0.0})
    for name in SAMPLE_SIZES:
        picked = [(test_id, reviewed[test_id]) for s, test_id, _ in sample if s == name]
        n, big_n = len(picked), tests_in[name]
        sampled_rows = sum(rows_of[t] for t, _ in picked)
        for cause in sorted(set(LABELS.values())):
            share = sum(1 for _, c in picked if c == cause) / n
            e = estimate[cause]
            e["tests"] += share * big_n
            e["tests_var"] += big_n * big_n * share * (1 - share) / n * (1 - n / big_n)
            e["rows"] += rows_in[name] * sum(rows_of[t] for t, c in picked if c == cause) / sampled_rows
    causes = {
        cause: {
            "tests_estimate": round(e["tests"], 1),
            "tests_95": round(1.96 * math.sqrt(e["tests_var"]), 1),
            "rows_estimate": round(e["rows"], 1),
        }
        for cause, e in sorted(estimate.items(), key=lambda kv: -kv[1]["rows"])
    }

    report = {
        "schema_version": 1,
        "census": {
            "commit": summary["commit"],
            "results": summary["results"],
            "profile_result_sha256": [p["result_sha256"] for p in summary["profiles"]],
        },
        "measured": {
            "rows": len(rows),
            "tests": len(rows_of),
            "rows_by_profile": dict(collections.Counter(r["profile"] for r in rows)),
            "rows_by_font_profile": dict(collections.Counter(r["font_profile"] for r in rows)),
            "tests_by_failing_scales": dict(collections.Counter(
                ",".join(sorted(s, key=float)) for s in scales.values()).most_common()),
            "strata": {name: {"tests": tests_in[name], "rows": rows_in[name]} for name in SAMPLE_SIZES},
        },
        "sample": [{"index": i, "stratum": s, "id": t, "profile": p, "cause": LABELS[i]}
                   for i, (s, t, p) in enumerate(sample)],
        "estimated": causes,
        "rows": rows,
    }
    args.output.write_text(json.dumps(report, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({k: report[k] for k in ("census", "measured", "estimated")}, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
