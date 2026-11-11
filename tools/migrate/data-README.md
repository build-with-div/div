# Div fidelity data

Generated evidence for [Div](https://github.com/build-with-div/div)'s
pixel comparison against Chrome for Testing: census results, the frozen
5,731-image archive, ported-test ledgers and older receipts. It was split out
of [zhuowcui/open-ui](https://github.com/zhuowcui/open-ui) so the main
repository stays small; the commit it came from is in `SOURCE.json`.

- `gates/`: the files Div's CI reads.
- `archive/`: older evidence that no gate reads.
- `MANIFEST.sha256`: SHA-256 and size of every file.

Each file sits at `<profile>/<path>`, where `<path>` is where Div's tools
expect it. Div pins one commit of this repository and the manifest's
SHA-256 in `fidelity-data.lock`; from a Div checkout,

```bash
python3 tools/fidelity_data.py fetch              # gates
python3 tools/fidelity_data.py fetch --profile all
```

clones that commit, checks every file against the manifest and restores the
files in place. Files here are never edited; new evidence arrives as new
files in a new commit, and Div moves its pin.
