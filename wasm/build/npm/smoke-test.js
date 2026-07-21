// @ts-check
// Smoke tests the packed @kaspa/sdk-wasm tarball.
//
// Installs the tarball into a throwaway consumer project and exercises the
// SDK offline, so the exact bytes that would ship to npm are what's tested.

const { execFileSync } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { workspaceVersion } = require('./assemble');

const wasmDir = path.resolve(__dirname, '..', '..');

/** @param {string} message @returns {never} */
function fail(message) {
  console.error(`smoke-test: ${message}`);
  process.exit(1);
}

/** @param {string} command @param {string[]} args @param {string} cwd */
function run(command, args, cwd) {
  execFileSync(command, args, { cwd, stdio: ['ignore', 'inherit', 'inherit'] });
}

let tarball = process.argv[2];
if (!tarball) {
  const releaseDir = path.join(wasmDir, 'npm-release');
  const tarballs = fs.existsSync(releaseDir) ? fs.readdirSync(releaseDir).filter((f) => f.endsWith('.tgz')) : [];
  if (tarballs.length !== 1) {
    fail(`expected exactly one .tgz in ${releaseDir}, found ${tarballs.length}; run 'bash assemble-npm' first`);
  }
  tarball = path.join(releaseDir, tarballs[0]);
}
tarball = path.resolve(tarball);
if (!fs.existsSync(tarball)) {
  fail(`no such tarball: ${tarball}`);
}

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'kaspa-sdk-wasm-smoke-'));
process.on('exit', () => fs.rmSync(tmpDir, { recursive: true, force: true }));

console.log(`installing ${path.basename(tarball)} into ${tmpDir}`);
fs.writeFileSync(path.join(tmpDir, 'package.json'), JSON.stringify({ name: 'smoke', private: true }, null, 2));
run('npm', ['install', '--no-audit', '--no-fund', tarball], tmpDir);

// the installed package must be the pure wasm-pack output with rewritten metadata
const installedDir = path.join(tmpDir, 'node_modules', '@kaspa', 'sdk-wasm');
const pkg = JSON.parse(fs.readFileSync(path.join(installedDir, 'package.json'), 'utf8'));
const version = workspaceVersion();

if (pkg.name !== '@kaspa/sdk-wasm') fail(`unexpected package name: ${pkg.name}`);
if (pkg.version !== version) fail(`package version ${pkg.version} != workspace version ${version}`);
if (pkg.dependencies && Object.keys(pkg.dependencies).length > 0) {
  fail(`package ships runtime dependencies: ${Object.keys(pkg.dependencies).join(', ')}`);
}
for (const file of ['kaspa.js', 'kaspa.d.ts', 'kaspa_bg.wasm', 'kaspa_bg.wasm.d.ts', 'README.md', 'LICENSE']) {
  if (!fs.existsSync(path.join(installedDir, file))) fail(`missing ${file} in installed package`);
}
console.log(`installed ${pkg.name}@${pkg.version}`);

// offline consumer: manual init on a server runtime
const consumerBody = `
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { initSync, Mnemonic, XPrv, createAddress, NetworkType, sompiToKaspaString } from '@kaspa/sdk-wasm';

const require = createRequire(import.meta.url);
initSync({ module: readFileSync(require.resolve('@kaspa/sdk-wasm/kaspa_bg.wasm')) });

const mnemonic = Mnemonic.random();
if (mnemonic.phrase.split(' ').length < 12) throw new Error('unexpected mnemonic: ' + mnemonic.phrase);

const xprv = new XPrv(mnemonic.toSeed());
const publicKey = xprv.derivePath("m/44'/111111'/0'/0/0").toXPub().toPublicKey();
const address = createAddress(publicKey, NetworkType.Mainnet);
if (!String(address).startsWith('kaspa:')) throw new Error('unexpected address: ' + address);

const kaspa = sompiToKaspaString(123_456_789n);
if (kaspa !== '1.23456789') throw new Error('unexpected sompi conversion: ' + kaspa);

console.log('derived address ' + address);
`;

fs.writeFileSync(path.join(tmpDir, 'main.mjs'), consumerBody);
run(process.execPath, ['main.mjs'], tmpDir);
console.log('ESM consumer: ok');

// Node >= 22.12 can require() ES modules, which is what keeps the web-target
// build usable from CommonJS consumers
const [major, minor] = process.versions.node.split('.').map(Number);
if (major > 22 || (major === 22 && minor >= 12)) {
  fs.writeFileSync(
    path.join(tmpDir, 'main.cjs'),
    `const { readFileSync } = require('node:fs');
const { initSync, Mnemonic } = require('@kaspa/sdk-wasm');
initSync({ module: readFileSync(require.resolve('@kaspa/sdk-wasm/kaspa_bg.wasm')) });
if (Mnemonic.random().phrase.split(' ').length < 12) throw new Error('unexpected mnemonic');
console.log('require() interop: ok');
`,
  );
  run(process.execPath, ['main.cjs'], tmpDir);
} else {
  console.log(`require() interop: skipped (needs Node >= 22.12, running ${process.versions.node})`);
}

console.log('smoke test passed');
