import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function run(command, args, capture = false) {
  if (capture) {
    return execFileSync(command, args, { cwd: root, encoding: 'utf8', stdio: ['inherit', 'pipe', 'inherit'] });
  }

  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${args.join(' ')} failed with exit code ${result.status}`);
}

try {
  if (process.platform !== 'win32' || process.arch !== 'x64') {
    throw new Error('Windows x64 is required to build these release assets.');
  }

  const { version } = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  const tauriVersion = JSON.parse(readFileSync(join(root, 'src-tauri', 'tauri.conf.json'), 'utf8')).version;
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version) || version !== tauriVersion) {
    throw new Error(`Package version (${version}) and Tauri version (${tauriVersion}) must match.`);
  }

  for (const [name, manifest] of [
    ['orca-tauri', join(root, 'src-tauri', 'Cargo.toml')],
    ['orca-core', join(root, 'crates', 'orca-core', 'Cargo.toml')],
  ]) {
    const metadata = JSON.parse(run('cargo', [
      'metadata', '--format-version', '1', '--no-deps', '--locked', '--offline',
      '--manifest-path', manifest,
    ], true));
    const crateVersion = metadata.packages.find((pkg) => pkg.name === name)?.version;
    if (crateVersion !== version) {
      throw new Error(`${name} version (${crateVersion ?? 'missing'}) must match ${version}.`);
    }
  }

  run(process.execPath, ['run', 'check']);
  run(process.execPath, ['run', 'tauri:build']);

  const buildDir = join(root, 'src-tauri', 'target', 'release');
  const outputDir = join(root, 'release', `v${version}`);
  const assets = [
    [join(buildDir, 'orca-tauri.exe'), `Orca_${version}_x64-portable.exe`],
    [join(buildDir, 'bundle', 'nsis', `Orca_${version}_x64-setup.exe`), `Orca_${version}_x64-setup.exe`],
  ];

  mkdirSync(outputDir, { recursive: true });
  for (const [source, filename] of assets) {
    statSync(source);
    copyFileSync(source, join(outputDir, filename));
  }

  console.log(`\nRelease assets: ${outputDir}`);
  for (const [, filename] of assets) console.log(`  ${filename}`);
} catch (error) {
  console.error(`Release build failed: ${error.message}`);
  process.exitCode = 1;
}
