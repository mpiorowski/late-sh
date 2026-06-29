// capture-frames.mjs — drive the genesis web UI through a walkthrough and save
// PNG frames for the README hero GIF (assembled by ffmpeg, see scripts header).
//   E2E_URL=http://localhost:8211/ node capture-frames.mjs <outDir>
import { chromium } from 'playwright-core';
import { mkdirSync } from 'node:fs';

const URL = process.env.E2E_URL || 'http://localhost:8211/';
const EXEC = process.env.E2E_CHROME || '/usr/bin/google-chrome';
const OUT = process.argv[2] || '/tmp/frames';
mkdirSync(OUT, { recursive: true });

const browser = await chromium.launch({ executablePath: EXEC, headless: true, args: ['--no-sandbox', '--disable-gpu', '--force-device-scale-factor=1'] });
const ctx = await browser.newContext({ viewport: { width: 1360, height: 800 }, deviceScaleFactor: 1 });
const page = await ctx.newPage();
const sleep = (ms) => page.waitForTimeout(ms);
let n = 0;
const shot = async (label) => { const f = `${OUT}/f${String(++n).padStart(2, '0')}.png`; await page.screenshot({ path: f }); console.log('frame', f, label); };

await page.goto(URL, { waitUntil: 'domcontentloaded' });
await page.waitForFunction(() => window.__ui && document.querySelector('#sideNav .side-item'), { timeout: 15000 });
await page.evaluate(() => { window.__ui.applyLayout('desktop'); window.__ui.applyTheme('dark'); });
await sleep(400);

// seed the thread with a real signed exchange
await page.fill('#input', 'is this plugin safe to run, or will it get me hacked?');
await page.click('#send');
await sleep(6500);
await shot('dark · general w/ agent reply');

await page.fill('#input', 'can you run the cve-bench benchmark and submit my score?');
await page.click('#send');
await sleep(5500);
await shot('dark · second exchange');

await page.evaluate(() => window.__ui.VIEWS.arena()); await sleep(700); await shot('dark · Arena');
await page.evaluate(() => window.__ui.VIEWS.retort()); await sleep(700); await shot('dark · Retort frontier');

await page.evaluate(() => { window.__ui.applyTheme('light'); window.__ui.VIEWS.market(); }); await sleep(700); await shot('light · Marketplace');
await page.evaluate(() => { window.__ui.applyTheme('aubergine'); window.__ui.VIEWS.console(); }); await sleep(700); await shot('aubergine · Console');

// custom theme (teal) on the board
await page.evaluate(() => { window.__ui.applyCustom({ base: 'dark', accent: '#00e5a0', bg: '#0b1020', panel: '#161d33', fg: '#eaf0ff' }); window.__ui.VIEWS.boards(); }); await sleep(700); await shot('custom teal · general');

// notifications modal
await page.evaluate(() => { window.__ui.notify('claude-agent replied in #general', 'agent'); window.__ui.notify('Applied Amber CRT theme', 'market'); window.__ui.notify('Looped in @graybeard', 'agent'); document.getElementById('bellBtn').click(); });
await sleep(600); await shot('custom · notifications modal');
await page.evaluate(() => document.getElementById('notifClose').click()); await sleep(300);

await page.evaluate(() => { window.__ui.applyTheme('dark'); window.__ui.VIEWS.boards(); }); await sleep(600); await shot('dark · general (loop)');

await browser.close();
console.log('captured', n, 'frames');
