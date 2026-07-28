import assert from 'node:assert/strict';
import { searchModKeyLabel, formatTime, prefersReducedMotion } from './utils.ts';

assert.equal(formatTime(0), '00:00');
assert.equal(formatTime(65), '01:05');
assert.equal(formatTime(600), '10:00');

// In Node there is no Mac UA by default → Alt
assert.equal(searchModKeyLabel(), 'Alt');

// prefers-reduced-motion: matchMedia may be missing in Node
assert.equal(typeof prefersReducedMotion(), 'boolean');

console.log('utils: ok');
