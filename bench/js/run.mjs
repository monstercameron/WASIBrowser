// JS DOM benchmark driver: vanilla DOM in Chromium via Playwright.
// Mirrors the GWB flow: create 1k -> update all -> update one -> class all
// -> clear -> create 5k -> update all -> clear. 5 repetitions, median-ish
// report (middle run after sorting by mutOnly).
import { chromium } from 'playwright';
import { fileURLToPath } from 'url';
import path from 'path';

const here = path.dirname(fileURLToPath(import.meta.url));
const REPS = 5;

function pick(results) {
  const sorted = [...results].sort((a, b) => a.mutOnlyUs - b.mutOnlyUs);
  return sorted[Math.floor(sorted.length / 2)];
}

const browser = await chromium.launch();
const page = await browser.newPage();
await page.goto('file://' + path.join(here, 'bench.html').replace(/\\/g, '/'));

const workloads = [
  ['create1k', () => page.evaluate(() => window.bench.create(1000))],
  ['updateAll@1k', () => page.evaluate(() => window.bench.updateAll())],
  ['updateOne@1k', () => page.evaluate(() => window.bench.updateOne())],
  ['classAll@1k', () => page.evaluate(() => window.bench.classAll())],
  ['clear@1k', () => page.evaluate(() => window.bench.clear())],
  ['create5k', () => page.evaluate(() => window.bench.create(5000))],
  ['updateAll@5k', () => page.evaluate(() => window.bench.updateAll())],
  ['clear@5k', () => page.evaluate(() => window.bench.clear())],
];

const report = {};
for (const [name, run] of workloads) report[name] = [];

for (let rep = 0; rep < REPS; rep++) {
  for (const [name, run] of workloads) {
    report[name].push(await run());
  }
}

console.log('workload, js_mut_only_us, js_forced_layout_us');
for (const [name] of workloads) {
  const r = pick(report[name]);
  console.log(`${name}, ${r.mutOnlyUs}, ${r.layoutUs}`);
}

await browser.close();
