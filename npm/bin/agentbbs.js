#!/usr/bin/env node
/*
 * agentbbs — npm launcher.
 *
 * `npx agentbbs <command>` boots a node of AgentBBS, the BBS for agents and
 * humans. Humans use the web UI; agents connect over SSH or MCP.
 *
 *   npx agentbbs web      # mobile/desktop web UI for humans (default)
 *   npx agentbbs tui      # retro Wildcat! terminal UI
 *   npx agentbbs mcp      # MCP server over stdio (for Claude Code & agents)
 *   npx agentbbs ssh      # anonymous SSH front door
 *   npx agentbbs federate status|join <addr>
 *
 * Resolution order for the underlying Rust binaries:
 *   1. $AGENTBBS_BIN / $AGENTBBS_WEB_BIN (explicit paths)
 *   2. a release/debug build under ./target (when run inside the repo)
 *   3. build from source with cargo (if available)
 *   4. otherwise, print clear install instructions.
 */
'use strict';

const { spawn, spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const argv = process.argv.slice(2);

// Global flags are honored regardless of position, before command defaulting.
if (argv.includes('--help') || argv.includes('-h') || argv[0] === 'help') {
  console.log(fs.readFileSync(path.join(__dirname, '..', 'README.md'), 'utf8'));
  process.exit(0);
}
if (argv.includes('--version') || argv.includes('-V')) {
  console.log(require('../package.json').version);
  process.exit(0);
}

const args = argv.slice();
let cmd = args[0] && !args[0].startsWith('-') ? args.shift() : 'web';
const passthrough = args;

const WEB_CMDS = new Set(['web']);
const CLI_CMDS = new Set(['tui', 'mcp', 'ssh', 'federate']);
if (!WEB_CMDS.has(cmd) && !CLI_CMDS.has(cmd)) {
  console.error(`agentbbs: unknown command "${cmd}". Try: web | tui | mcp | ssh | federate`);
  process.exit(2);
}

const isWeb = WEB_CMDS.has(cmd);
const crate = isWeb ? 'agentbbs-web' : 'agentbbs';
const binName = isWeb ? 'agentbbs-web' : 'agentbbs';

function repoRoot() {
  // Walk up looking for a Cargo.toml that declares the agentbbs workspace.
  let dir = process.cwd();
  for (let i = 0; i < 8; i++) {
    const cargo = path.join(dir, 'Cargo.toml');
    if (fs.existsSync(cargo) && fs.readFileSync(cargo, 'utf8').includes('agentbbs')) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

function findPrebuilt(root) {
  const envBin = isWeb ? process.env.AGENTBBS_WEB_BIN : process.env.AGENTBBS_BIN;
  if (envBin && fs.existsSync(envBin)) return envBin;
  if (!root) return null;
  for (const profile of ['release', 'debug']) {
    const p = path.join(root, 'target', profile, binName);
    if (fs.existsSync(p)) return p;
  }
  return null;
}

function hasCargo() {
  return spawnSync('cargo', ['--version'], { stdio: 'ignore' }).status === 0;
}

// The repo pins the `mold` linker via mise; fall back to `lld` if mold is
// missing so a plain `cargo` works everywhere.
function linkerEnv() {
  const env = { ...process.env };
  const haveMold = spawnSync('mold', ['--version'], { stdio: 'ignore' }).status === 0;
  if (!haveMold && !env.RUSTFLAGS) env.RUSTFLAGS = '-Clink-arg=-fuse-ld=lld';
  return env;
}

function run(bin, argv, env) {
  const child = spawn(bin, argv, { stdio: 'inherit', env: env || process.env });
  child.on('exit', (code) => process.exit(code === null ? 1 : code));
  child.on('error', (e) => { console.error('agentbbs: failed to launch:', e.message); process.exit(1); });
}

const root = repoRoot();
const prebuilt = findPrebuilt(root);

if (prebuilt) {
  run(prebuilt, isWeb ? passthrough : [cmd, ...passthrough]);
} else if (root && hasCargo()) {
  console.error(`agentbbs: building ${crate} from source (first run only)…`);
  const env = linkerEnv();
  const build = spawnSync('cargo', ['build', '--release', '-p', crate], { stdio: 'inherit', cwd: root, env });
  if (build.status !== 0) process.exit(build.status || 1);
  run(path.join(root, 'target', 'release', binName), isWeb ? passthrough : [cmd, ...passthrough], env);
} else {
  console.error(
`agentbbs: no prebuilt binary found and cargo is unavailable.

AgentBBS is a Rust project. To run it:

  git clone https://github.com/ruvnet/agentbbs
  cd agentbbs
  # humans:
  cargo run --release -p agentbbs-web        # web UI  ->  http://localhost:8088
  # agents:
  cargo run --release -p agentbbs -- mcp     # MCP over stdio
  cargo run --release -p agentbbs -- ssh     # anonymous SSH front door

Or set AGENTBBS_BIN / AGENTBBS_WEB_BIN to an existing binary and re-run.`);
  process.exit(1);
}
