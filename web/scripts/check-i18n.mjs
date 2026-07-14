import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

const root = new URL('..', import.meta.url).pathname;
const sourceRoot = join(root, 'src');
const locales = {
  en: JSON.parse(readFileSync(join(root, 'locales/en.json'), 'utf8')),
  'zh-CN': JSON.parse(readFileSync(join(root, 'locales/zh-CN.json'), 'utf8')),
};

const keys = new Set();
for (const file of walk(sourceRoot)) {
  if (!/\.(ts|vue)$/.test(file)) continue;
  const text = readFileSync(file, 'utf8');
  for (const match of text.matchAll(/\b(?:t|\$t)\(\s*['"]([^'"]+)['"]/g)) {
    keys.add(match[1]);
  }
}

const missing = [];
for (const key of keys) {
  for (const [locale, messages] of Object.entries(locales)) {
    if (!hasKey(messages, key)) missing.push(`${locale}:${key}`);
  }
}

if (missing.length > 0) {
  console.error(`Missing i18n keys:\n${missing.join('\n')}`);
  process.exit(1);
}

console.log(`i18n ok: ${keys.size} keys checked`);

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) yield* walk(path);
    else yield path;
  }
}

function hasKey(obj, key) {
  return key.split('.').every((part) => {
    if (obj && Object.prototype.hasOwnProperty.call(obj, part)) {
      obj = obj[part];
      return true;
    }
    return false;
  });
}
