#!/usr/bin/env python3
"""Add a pull request to the merge queue.

    python tools/enqueue_pr.py <number> [--repo build-with-div/div]

`gh pr merge <number> --merge` cannot queue a pull request here: gh asks
GitHub to enable auto-merge, which this repository does not allow. This
script runs the `enqueuePullRequest` GraphQL mutation behind the "Add to
merge queue" button instead, for the pull request's node id and its current
head commit (`gh pr view <number> --json id,headRefOid`). GitHub refuses the
request if the head moved after that lookup, so a later push is never queued
unseen. Authentication is gh's own (or GH_TOKEN).
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys

MUTATION = (
    "mutation($id: ID!, $sha: GitObjectID) {"
    " enqueuePullRequest(input: {pullRequestId: $id, expectedHeadOid: $sha}) {"
    " mergeQueueEntry { state position } } }"
)


def gh(*args: str) -> str:
    result = subprocess.run(["gh", *args], capture_output=True, text=True, encoding="utf-8")
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        raise SystemExit(f"enqueue_pr: gh {args[0]} failed")
    return result.stdout


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("number", type=int, help="pull request number")
    parser.add_argument("--repo", default="build-with-div/div")
    args = parser.parse_args(argv)

    pr = json.loads(gh("pr", "view", str(args.number), "--repo", args.repo,
                       "--json", "id,headRefOid,state"))
    if pr["state"] != "OPEN":
        raise SystemExit(f"enqueue_pr: #{args.number} is {pr['state'].lower()}")
    reply = json.loads(gh("api", "graphql", "-f", f"query={MUTATION}",
                          "-f", f"id={pr['id']}", "-f", f"sha={pr['headRefOid']}"))
    entry = reply["data"]["enqueuePullRequest"]["mergeQueueEntry"]
    print(f"#{args.number} at {pr['headRefOid'][:12]}: {entry['state'].lower()}, "
          f"position {entry['position']}")


if __name__ == "__main__":
    main()
