// @ts-check
// Smoke tests the packed @kaspa/sdk-wasm tarball.
//
// Installs the tarball into a throwaway consumer project and exercises the
// SDK offline, so the exact bytes that would ship to npm are what's tested.

const { spawnSync } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { workspaceVersion, Failure } = require('./assemble');

/** @param {string} command @param {string[]} args @param {string} cwd */
function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, stdio: ['ignore', 'inherit', 'inherit'] });
  if (result.error) throw new Failure(`command failed: ${command} ${args.join(' ')} (${result.error.message})`);
  if (result.status !== 0) {
    const reason = result.status !== null ? `exit ${result.status}` : `signal ${result.signal}`;
    throw new Failure(`command failed: ${command} ${args.join(' ')} (${reason})`);
  }
}

function main() {
  if (process.argv.length !== 3) {
    throw new Failure(`usage: node smoke-test.js <tarball.tgz> (got ${process.argv.length - 2} arguments)`);
  }
  const tarball = path.resolve(process.argv[2]);
  if (!fs.existsSync(tarball)) {
    throw new Failure(`no such tarball: ${tarball}`);
  }

  using tmp = fs.mkdtempDisposableSync(path.join(os.tmpdir(), 'kaspa-sdk-wasm-smoke-'));
  const tmpDir = tmp.path;

  console.log(`installing ${path.basename(tarball)} into ${tmpDir}`);
  fs.writeFileSync(path.join(tmpDir, 'package.json'), JSON.stringify({ name: 'smoke', private: true }, null, 2));
  run('npm', ['install', '--no-audit', '--no-fund', tarball], tmpDir);

  // the installed package must be the pure wasm-pack output with rewritten metadata
  const installedDir = path.join(tmpDir, 'node_modules', '@kaspa', 'sdk-wasm');
  const pkg = JSON.parse(fs.readFileSync(path.join(installedDir, 'package.json'), 'utf8'));
  const version = workspaceVersion();

  if (pkg.name !== '@kaspa/sdk-wasm') throw new Failure(`unexpected package name: ${pkg.name}`);
  if (pkg.version !== version) throw new Failure(`package version ${pkg.version} != workspace version ${version}`);
  if (pkg.dependencies && Object.keys(pkg.dependencies).length > 0) {
    throw new Failure(`package ships runtime dependencies: ${Object.keys(pkg.dependencies).join(', ')}`);
  }
  for (const file of ['kaspa.js', 'kaspa.d.ts', 'kaspa_bg.wasm', 'kaspa_bg.wasm.d.ts', 'README.md', 'LICENSE']) {
    if (!fs.existsSync(path.join(installedDir, file))) throw new Failure(`missing ${file} in installed package`);
  }
  console.log(`installed ${pkg.name}@${pkg.version}`);

  // offline consumer: manual init on a server runtime
  fs.writeFileSync(path.join(tmpDir, 'main.mjs'), `
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { initSync, Mnemonic, sompiToKaspaString } from '@kaspa/sdk-wasm';

const require = createRequire(import.meta.url);
initSync({ module: readFileSync(require.resolve('@kaspa/sdk-wasm/kaspa_bg.wasm')) });

const kaspa = sompiToKaspaString(123_456_789n);
if (kaspa !== '1.23456789') throw new Error('unexpected sompi conversion: ' + kaspa);

// Mnemonic.random() exercises the entropy binding (crypto.getRandomValues),
// which nothing deterministic touches
if (Mnemonic.random().phrase.split(' ').length < 12) throw new Error('unexpected mnemonic');
`);
  run(process.execPath, ['main.mjs'], tmpDir);
  console.log('ESM consumer: ok');

  // require() of ES modules (Node >= 22.12) is what keeps the web-target build
  // usable from CommonJS consumers
  fs.writeFileSync(path.join(tmpDir, 'main.cjs'), `
const { readFileSync } = require('node:fs');
const { initSync, sompiToKaspaString } = require('@kaspa/sdk-wasm');

initSync({ module: readFileSync(require.resolve('@kaspa/sdk-wasm/kaspa_bg.wasm')) });

const kaspa = sompiToKaspaString(123_456_789n);
if (kaspa !== '1.23456789') throw new Error('unexpected sompi conversion: ' + kaspa);
`);
  run(process.execPath, ['main.cjs'], tmpDir);
  console.log('require() interop: ok');

  console.log('smoke test passed');
}

try {
  main();
} catch (err) {
  if (!(err instanceof Failure)) throw err;
  console.error(`smoke-test: ${err.message}`);
  process.exitCode = 1;
}
