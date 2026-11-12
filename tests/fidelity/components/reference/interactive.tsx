// Browser entry of the trace pages: render the page's trace fixture with
// React, so Radix's behavior runs in Chromium. `build.mjs` bundles this
// file into `pages/interactive.js`.
//
// The page holds the fixture's static markup, which Tailwind scans for
// classes; React replaces it with a live tree. `window.__divTraceReady` is
// set once React has committed and its effects have run.

import { flushSync } from "react-dom"
import { createRoot } from "react-dom/client"

import { traceFixtures } from "./fixtures"

declare global {
  interface Window {
    __divTraceReady?: boolean
  }
}

const main = document.querySelector("main[data-fixture]")
if (!main) throw new Error("the trace page has no <main data-fixture>")
const id = main.getAttribute("data-fixture")
const fixture = traceFixtures.find((candidate) => candidate.id === id)
if (!fixture) throw new Error(`no trace fixture ${id}`)

const root = createRoot(main)
flushSync(() => root.render(fixture.render()))
requestAnimationFrame(() =>
  setTimeout(() => {
    window.__divTraceReady = true
  }, 0)
)
