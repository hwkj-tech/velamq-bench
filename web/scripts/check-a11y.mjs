import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = new URL('..', import.meta.url).pathname;
const sourceRoot = join(root, 'src');
const issues = [];

for (const file of walk(sourceRoot)) {
  if (!file.endsWith('.vue')) continue;
  const text = readFileSync(file, 'utf8');
  checkImages(file, text);
  checkLinks(file, text);
  checkIconButtons(file, text);
  checkTabs(file, text);
}

if (issues.length > 0) {
  console.error(`a11y smoke failed:\n${issues.join('\n')}`);
  process.exit(1);
}

console.log('a11y smoke ok');

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) yield* walk(path);
    else yield path;
  }
}

function checkImages(file, text) {
  for (const match of text.matchAll(/<img\b[^>]*>/gi)) {
    if (!/\b:?alt=/.test(match[0])) report(file, text, match.index, '<img> is missing alt text');
  }
}

function checkLinks(file, text) {
  for (const match of text.matchAll(/<a\b[^>]*href=["']#["'][^>]*>/gi)) {
    report(file, text, match.index, 'Anchor uses href="#"');
  }
}

function checkIconButtons(file, text) {
  for (const match of text.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/gi)) {
    const attrs = match[1];
    const inner = match[2].trim();
    if (/\b(?:aria-label|:aria-label|v-bind:aria-label|title|:title|v-bind:title)=/.test(attrs)) continue;
    const textContent = inner
      .replace(/<[^>]+>/g, '')
      .replace(/{{[\s\S]*?}}/g, 'translated')
      .replace(/&nbsp;/g, ' ')
      .trim();
    if (!textContent) report(file, text, match.index, 'Icon-only button needs aria-label or title');
  }
}

function checkTabs(file, text) {
  for (const match of text.matchAll(/<[^>]+\brole=["']tab["'][^>]*>/gi)) {
    if (!/\b:?aria-selected=/.test(match[0])) report(file, text, match.index, 'role="tab" needs aria-selected');
  }
}

function report(file, text, index, message) {
  const line = text.slice(0, index).split('\n').length;
  issues.push(`${relative(root, file)}:${line} ${message}`);
}
