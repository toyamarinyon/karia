import { parse } from '@babel/parser';
import * as traverseModule from '@babel/traverse';
import type { NodePath } from '@babel/traverse';
import type * as t from '@babel/types';

export type ModuleAccess = { specifier: string; name: string; start: number; end: number };
const traverseNs: any = traverseModule as any;
const traverseFn: any = traverseNs.default?.default ?? traverseNs.default ?? traverseNs['module.exports'] ?? traverseNs;

function relativeCss(value: unknown): value is string {
  return typeof value === 'string' && /^(?:\.\.?\/).*\.module\.css$/.test(value);
}

/** Resolve a CSS-module property at a UTF-16 cursor, respecting lexical bindings. */
export function moduleAccess(text: string, offset: number): ModuleAccess | undefined {
  const cursor = Math.max(0, Math.min(offset, text.length));
  let ast: t.File;
  try {
    ast = parse(text, { sourceType: 'module', errorRecovery: true, plugins: ['typescript', 'jsx'], ranges: true });
  } catch {
    // Babel can reject a partially typed construct despite errorRecovery. A sentinel
    // keeps the parser in the expression grammar without changing the returned offsets.
    try { ast = parse(text.slice(0, cursor) + '__css_lsp_cursor__' + text.slice(cursor), { sourceType: 'module', errorRecovery: true, plugins: ['typescript', 'jsx'], ranges: true }); }
    catch { return undefined; }
  }
  let result: ModuleAccess | undefined;
  const imported = new Map<string, string>();
  traverseFn(ast, {
    ImportDeclaration(p: any) {
      const source = p.node.source.value;
      if (!relativeCss(source)) return;
      const spec = p.get('specifiers');
      for (const s of spec) if (s.isImportDefaultSpecifier()) imported.set(s.node.local.name, source);
    },
    MemberExpression(p: NodePath<t.MemberExpression>) {
      if (result) return;
      const object = p.get('object');
      if (!object.isIdentifier()) return;
      const binding = p.scope.getBinding(object.node.name);
      const source = binding?.path.isImportDefaultSpecifier() ? imported.get(object.node.name) : undefined;
      if (!source) return;
      const property = p.get('property');
      let name = ''; let start = cursor; let end = cursor;
      if (!p.node.computed && property.isIdentifier()) {
        name = property.node.name; start = property.node.start ?? cursor; end = property.node.end ?? start + name.length;
        if (name === '__css_lsp_cursor__' && start >= cursor) { name = ''; start = cursor; end = cursor; }
      } else if (p.node.computed && property.isStringLiteral()) {
        name = property.node.value; start = (property.node.start ?? cursor) + 1; end = (property.node.end ?? start + name.length) - 1;
      } else return;
      if (cursor < start || cursor > end) return;
      result = { specifier: source, name, start, end };
    },
    Identifier(p: any) {
      if (result || !imported.has(p.node.name)) return;
      const binding = p.scope.getBinding(p.node.name);
      if (!binding?.path.isImportDefaultSpecifier()) return;
      // Incomplete `styles.` has no MemberExpression yet. The identifier must be
      // immediately followed by a dot at the cursor (comments/strings are excluded by AST).
      const after = text.slice(p.node.end ?? 0, cursor + 1);
      if (cursor >= (p.node.end ?? 0) && /^\s*\.$/.test(after)) result = { specifier: imported.get(p.node.name)!, name: '', start: cursor, end: cursor };
    },
  });
  return result;
}
