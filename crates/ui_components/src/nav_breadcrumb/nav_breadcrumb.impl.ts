// Browser behavior of <nav-breadcrumb>; mirrors the Rust NavBreadcrumbLogic
// impl in mod.rs; keep both in sync.
import type { NavBreadcrumb } from './nav-breadcrumb.js';

type Crumb = { label: string; href: string; sep: string; plain: boolean };

// The display rows: `sep` is empty on the first crumb and the divider
// afterwards; `plain` complements `href`.
export function crumbs(el: NavBreadcrumb): Crumb[] {
  const items = Array.isArray(el.items) ? el.items : [];
  return items.map((item, index) => {
    const row = (item ?? {}) as Record<string, unknown>;
    const label = typeof row.label === 'string' ? row.label : '';
    const href = typeof row.href === 'string' ? row.href : '';
    return {
      label,
      href,
      sep: index === 0 ? '' : el.divider,
      plain: href === '',
    };
  });
}
