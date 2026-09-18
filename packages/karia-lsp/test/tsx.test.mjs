import test from 'node:test';
import assert from 'node:assert/strict';
import { moduleAccess } from '../dist/tsx.js';
const at = (s, n, k = 0) => s.indexOf(n) + k;
test('default CSS module access and multiple imports', () => {
  const s = `import styles from './a.module.css';\nimport other from './b.module.css';\nstyles.page;`;
  const r = moduleAccess(s, at(s, 'page') + 2);
  assert.deepEqual(r, { specifier: './a.module.css', name: 'page', start: at(s, 'page'), end: at(s, 'page') + 4 });
  assert.equal(moduleAccess(s, at(s, 'other')), undefined);
});
test('bracket access, incomplete dot, and shadowing', () => {
  const a = `import styles from './a.module.css';\nstyles["hero"];`;
  assert.equal(moduleAccess(a, at(a, 'hero') + 1).name, 'hero');
  const b = `import styles from './a.module.css';\nstyles.`;
  assert.equal(moduleAccess(b, b.length).name, '');
  const c = `import styles from './a.module.css';\nfunction f(styles) { return styles.page }`;
  assert.equal(moduleAccess(c, c.lastIndexOf('page') + 1), undefined);
});
test('ignores comments and strings, supports JSX and middle cursor', () => {
  const s = `import styles from './a.module.css';\n// styles.fake\nconst x = "styles.nope";\nreturn <div className={styles.page}/>;`;
  assert.equal(moduleAccess(s, at(s, 'fake') + 2), undefined);
  assert.equal(moduleAccess(s, at(s, 'nope') + 2), undefined);
  assert.equal(moduleAccess(s, at(s, 'page') + 2).name, 'page');
});
test('supports incomplete JSX member access', () => {
  const s = `import styles from './a.module.css';\nconst x = <div className={styles.} />;`;
  const r = moduleAccess(s, s.indexOf('styles.') + 'styles.'.length);
  assert.deepEqual(r, { specifier: './a.module.css', name: '', start: s.indexOf('styles.') + 'styles.'.length, end: s.indexOf('styles.') + 'styles.'.length });
});
test('incomplete dot before a following statement', () => {
  const s = `import styles from './a.module.css';\n\nstyles.\nexport function f() { return styles.page; }`;
  const cursor = s.indexOf('styles.\n') + 'styles.'.length;
  assert.deepEqual(moduleAccess(s, cursor), { specifier: './a.module.css', name: '', start: cursor, end: cursor });
  const partial = s.replace('styles.\n', 'styles.pa\n');
  const r = moduleAccess(partial, partial.indexOf('styles.pa') + 'styles.pa'.length);
  assert.equal(r?.name, 'pa');
});
