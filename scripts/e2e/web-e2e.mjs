// End-to-end browser test for the AgentBBS web UI (ADR-0024 + Console panel).
//
// Drives a real Chromium against a served copy of the app and asserts the full
// feature surface: boot, both layouts, all six themes, posting (sign + verify)
// with an agent reply, the community panels, the Console/debug panel, and the
// absence of console errors.
//
// Usage:  E2E_URL=http://localhost:8211/ node web-e2e.mjs
// Chrome: set E2E_CHROME to a chromium/chrome executable, or rely on the
//         CHROME/PUPPETEER/Playwright defaults. In CI we install one.
import { chromium } from 'playwright-core';

const URL = process.env.E2E_URL || 'http://localhost:8211/';
const EXEC = process.env.E2E_CHROME || process.env.CHROME_PATH || '/usr/bin/google-chrome';
const HEADLESS = process.env.E2E_HEADFUL ? false : true;

const results = [];
const ok = (cond, msg) => { results.push({ pass: !!cond, msg }); console.log(`${cond ? 'PASS' : 'FAIL'}  ${msg}`); };

const browser = await chromium.launch({ executablePath: EXEC, headless: HEADLESS, args: ['--no-sandbox', '--disable-gpu'] });
const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
const page = await ctx.newPage();

const consoleErrors = [];
// Ignore benign favicon 404s (not an app error); everything else counts.
page.on('console', m => { if (m.type() === 'error' && !/favicon/i.test(m.text())) consoleErrors.push(m.text()); });
page.on('pageerror', e => consoleErrors.push('pageerror: ' + e.message));

try {
  await page.goto(URL, { waitUntil: 'domcontentloaded' });
  // boot: __ui + sidebar populated
  await page.waitForFunction(() => window.__ui && document.querySelector('#sideNav .side-item'), { timeout: 15000 });
  ok(true, 'app booted (window.__ui + sidebar present)');

  // ---- desktop layout ----
  await page.evaluate(() => window.__ui.applyLayout('desktop'));
  ok(await page.evaluate(() => document.documentElement.dataset.layout) === 'desktop', 'desktop layout applied');
  const chans = await page.evaluate(() => document.querySelectorAll('#sideNav [data-nav^="board:"]').length);
  ok(chans >= 3, `sidebar shows ${chans} channels`);
  ok(await page.evaluate(() => getComputedStyle(document.getElementById('rightbar')).display !== 'none'), 'right rail visible on desktop');

  // ---- themes ----
  const themes = await page.evaluate(() => window.__ui.THEMES.map(t => t.id));
  ok(themes.length === 6, `theme registry has ${themes.length} themes: ${themes.join(',')}`);
  const bgs = new Set();
  for (const t of themes) {
    await page.evaluate((id) => window.__ui.applyTheme(id), t);
    const applied = await page.evaluate(() => document.documentElement.dataset.theme);
    const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    bgs.add(bg);
    ok(applied === t, `theme '${t}' applied (body bg ${bg})`);
  }
  ok(bgs.size >= 4, `themes produce distinct backgrounds (${bgs.size} unique)`);
  await page.evaluate(() => window.__ui.applyTheme('dark'));

  // ---- posting: sign + verify + agent reply ----
  await page.fill('#input', '@graybeard is this signed message verifiable?');
  await page.click('#send');
  await page.waitForFunction(() => [...document.querySelectorAll('.row.me .bubble')].some(b => /verifiable/.test(b.textContent)), { timeout: 10000 });
  ok(true, 'posted message appears in thread');
  ok(await page.evaluate(() => [...document.querySelectorAll('.row.me .sig')].some(s => !s.classList.contains('bad'))), 'message is signed + verified (✓ signed)');
  // A real reply, not the transient #thinking placeholder (also a .row.them).
  const realReply = () => [...document.querySelectorAll('.row.them')].some(r => r.id !== 'thinking' && !/thinking…/.test(r.textContent));
  await page.waitForFunction(realReply, { timeout: 12000 }).catch(() => {});
  ok(await page.evaluate(realReply), 'agent reply rendered');
  // Let the async reply fully settle (no #thinking) before navigating away.
  await page.waitForFunction(() => !document.getElementById('thinking'), { timeout: 8000 }).catch(() => {});

  // ---- community: Arena (sidebar) ----
  await page.click('[data-nav="view:arena"]');
  await page.waitForTimeout(300);
  ok(await page.evaluate(() => /CVE-Bench|Arena/.test(document.getElementById('thread').textContent)), 'Arena view renders');
  ok(await page.evaluate(() => document.querySelector('[data-nav="view:arena"]').classList.contains('active')), 'Arena sidebar item active-highlighted');

  // ---- community: Retort (frontier plot) ----
  await page.click('[data-nav="view:retort"]');
  await page.waitForTimeout(300);
  ok(await page.evaluate(() => /Retort|Pareto|frontier/i.test(document.getElementById('thread').textContent) && !!document.querySelector('#thread svg')), 'Retort view renders with frontier plot');

  // ---- Console / debug panel ----
  await page.click('[data-nav="view:console"]');
  await page.waitForTimeout(200);
  ok(await page.evaluate(() => /diagnostics & live console/.test(document.getElementById('thread').textContent)), 'Console panel renders diagnostics');
  ok(await page.evaluate(() => /console capture armed/.test(document.getElementById('thread').textContent)), 'Console panel mirrors captured console output');
  ok(await page.evaluate(() => !!document.getElementById('dbg-ping')), 'Console panel has debug controls');
  ok(await page.evaluate(() => typeof window.__dbg === 'object' && Array.isArray(window.__dbg.log)), 'window.__dbg ring buffer exposed');

  // ---- mobile layout + persistence ----
  await page.evaluate(() => window.__ui.applyLayout('mobile'));
  ok(await page.evaluate(() => document.documentElement.dataset.layout === 'mobile' && getComputedStyle(document.getElementById('sidebar')).display === 'none'), 'mobile layout hides sidebar');
  ok(await page.evaluate(() => localStorage.getItem('agentbbs.layout') === 'mobile' && !!localStorage.getItem('agentbbs.theme')), 'layout + theme persisted to localStorage');

  // ---- no console errors throughout ----
  ok(consoleErrors.length === 0, `zero console errors${consoleErrors.length ? ' -> ' + consoleErrors.slice(0, 5).join(' | ') : ''}`);
} catch (e) {
  ok(false, 'test harness error: ' + e.message);
} finally {
  await browser.close();
}

const failed = results.filter(r => !r.pass);
console.log(`\n${results.length - failed.length}/${results.length} checks passed`);
process.exit(failed.length ? 1 : 0);
