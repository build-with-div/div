// Builds the coming-soon cover into dist-cover/: cover/index.html and the
// files it uses, taken from where scripts/sync-assets.mjs copied them after
// checking their SHA-256 (run that first; `npm run build:cover` does).
//
// GitHub Pages publishes dist-cover/ or the full site's dist/, whichever
// PAGES_DIR names in .github/workflows/site.yml (see site/README.md).

import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const siteRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const out = join(siteRoot, 'dist-cover');

// [source in site/, path in dist-cover/]
const files = [
  ['cover/index.html', 'index.html'],
  ['public/favicon.svg', 'favicon.svg'],
  ['public/favicon.ico', 'favicon.ico'],
  ['public/apple-touch-icon.png', 'apple-touch-icon.png'],
  ['src/assets/generated/brand/div-wordmark.svg', 'div-wordmark.svg'],
  ['src/assets/generated/brand/div-wordmark-dark.svg', 'div-wordmark-dark.svg'],
  ['src/assets/generated/fonts/Geist-Variable.woff2', 'fonts/Geist-Variable.woff2'],
  ['public/fonts/Geist-OFL.txt', 'fonts/Geist-OFL.txt'],
  ['public/CNAME', 'CNAME'],
];

rmSync(out, { recursive: true, force: true });
for (const [source, dest] of files) {
  const target = join(out, dest);
  mkdirSync(dirname(target), { recursive: true });
  copyFileSync(join(siteRoot, source), target);
}

// The page must stay out of search results and refer only to its own files.
const html = readFileSync(join(out, 'index.html'), 'utf8');
const problems = [];
if (!html.includes('<meta name="robots" content="noindex" />')) {
  problems.push('index.html has no <meta name="robots" content="noindex">');
}
const urls = [
  ...html.matchAll(/\s(?:href|src|srcset)="([^"]+)"/g),
  ...html.matchAll(/url\('([^']+)'\)/g),
].map((m) => m[1]);
for (const url of urls) {
  if (/^[a-z]+:|^\/\//i.test(url) || url.startsWith('/')) {
    problems.push(`index.html links outside the cover: ${url}`);
  } else if (!existsSync(join(out, url))) {
    problems.push(`index.html refers to a file the cover does not have: ${url}`);
  }
}
if (problems.length) {
  console.error(`Cover check failed:\n  ${problems.join('\n  ')}`);
  process.exit(1);
}
console.log(`Cover built in dist-cover/: ${files.length} files, ${urls.length} references checked.`);
