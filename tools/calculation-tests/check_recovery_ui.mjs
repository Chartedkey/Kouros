// Verify the actual tile's null/zero/positive contract without the native runtime.
import { build } from 'esbuild';
import { createRequire } from 'node:module';
import { mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

const root = fileURLToPath(new URL('../../', import.meta.url));
const output = fileURLToPath(new URL('./target/recovery-ui.cjs', import.meta.url));
await mkdir(path.dirname(output), { recursive: true });
await build({ absWorkingDir: root, entryPoints: ['src/pages/TrainingParts.tsx'],
  outfile: output, bundle: true, platform: 'node', format: 'cjs', jsx: 'automatic',
  external: ['react', 'react-dom'], logLevel: 'silent' });
const { RecoveryTile } = createRequire(import.meta.url)(output);
for (const [recovery, expected] of [
  [undefined, 'нет оценки'],
  [{ status: 'unknown', leftH: null, reasons: ['Недостаточно истории'] }, 'нет оценки'],
  [{ status: 'clear', leftH: 0 }, 'нет остатка'],
  [{ status: 'estimated', leftH: 12 }, '12'],
]) {
  const markup = renderToStaticMarkup(React.createElement(RecoveryTile, { s: { recovery } }));
  assert(markup.includes(expected), markup);
  assert(!markup.includes('готов'), markup);
  assert(!markup.includes('до '), markup);
}
console.log('PASS: recovery tile distinguishes unknown/zero/positive; no physiological clearance or fabricated clock.');
