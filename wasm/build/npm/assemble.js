// @ts-check
// Assembles the publishable @kaspa/sdk-wasm npm package from the wasm-pack
// output in web/kaspa.
//
// Invoked by `assemble-npm`, which builds web/kaspa and packs the result.

const fs = require('fs');
const path = require('path');

const wasmDir = path.resolve(__dirname, '..', '..');
const sourceDir = path.join(wasmDir, 'web', 'kaspa');
const releaseDir = path.join(wasmDir, 'npm-release');
const stagingDir = path.join(releaseDir, 'sdk-wasm');

const BUILD_FILES = ['kaspa.js', 'kaspa.d.ts', 'kaspa_bg.wasm', 'kaspa_bg.wasm.d.ts'];

/** @param {string} message @returns {never} */
function fail(message) {
  console.error(`assemble-npm: ${message}`);
  process.exit(1);
}

function workspaceVersion() {
  const cargoTomlPath = path.join(wasmDir, '..', 'Cargo.toml');
  const cargoToml = fs.readFileSync(cargoTomlPath, 'utf8');
  const section = cargoToml.split(/^\[workspace\.package\]\s*$/m)[1];
  const match = section && section.split(/^\[/m)[0].match(/^version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error(`unable to parse [workspace.package] version from ${cargoTomlPath}`);
  }
  return match[1];
}

function assemble() {
  if (!fs.existsSync(path.join(sourceDir, 'package.json'))) {
    fail(`missing wasm-pack output at ${sourceDir}\n` + "run 'bash assemble-npm' or 'bash build-release' first");
  }

  const pkg = JSON.parse(fs.readFileSync(path.join(sourceDir, 'package.json'), 'utf8'));
  const version = workspaceVersion();

  if (pkg.version !== version) {
    fail(`web/kaspa is stale: built version ${pkg.version} != workspace version ${version}\n` + "rebuild with 'bash assemble-npm'");
  }

  for (const file of BUILD_FILES) {
    if (!fs.existsSync(path.join(sourceDir, file))) {
      fail(`missing ${file} in ${sourceDir}`);
    }
  }

  pkg.name = '@kaspa/sdk-wasm';
  pkg.homepage = 'https://github.com/kaspanet/rusty-kaspa#readme';
  pkg.bugs = { url: 'https://github.com/kaspanet/rusty-kaspa/issues' };
  pkg.keywords = ['kaspa', 'wasm', 'sdk', 'blockdag', 'wallet', 'rpc'];
  pkg.publishConfig = { access: 'public' };
  if (!pkg.files.includes('kaspa_bg.wasm.d.ts')) {
    pkg.files.push('kaspa_bg.wasm.d.ts');
  }

  fs.rmSync(releaseDir, { recursive: true, force: true });
  fs.mkdirSync(stagingDir, { recursive: true });

  for (const file of BUILD_FILES) {
    fs.copyFileSync(path.join(sourceDir, file), path.join(stagingDir, file));
  }

  fs.copyFileSync(path.join(__dirname, 'README.md'), path.join(stagingDir, 'README.md'));
  fs.copyFileSync(path.join(wasmDir, 'LICENSE'), path.join(stagingDir, 'LICENSE'));
  fs.writeFileSync(path.join(stagingDir, 'package.json'), JSON.stringify(pkg, null, 2) + '\n');

  console.log(`assembled ${pkg.name}@${pkg.version} at ${path.relative(process.cwd(), stagingDir)}`);
}

if (require.main === module) {
  assemble();
}

module.exports = { workspaceVersion };
