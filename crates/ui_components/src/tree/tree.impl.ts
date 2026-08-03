// The browser logic twin of `mod.rs`; keep both walks in sync.

type TreeNode = { id?: string; label?: string; children?: TreeNode[] };

type TreeHost = {
    nodes: TreeNode[];
    expanded: string[];
    selected: string;
};

const childrenOf = (node: TreeNode): TreeNode[] => node.children ?? [];

/** Depth-first over the expanded subtrees; level is 1-based like `aria-level`. */
export function rows(el: TreeHost): object[] {
    const open = new Set(el.expanded ?? []);
    const out: object[] = [];
    const walk = (nodes: TreeNode[], level: number) => {
        for (const node of nodes) {
            const id = node.id ?? '';
            const branch = childrenOf(node).length > 0;
            const expanded = branch && open.has(id);
            out.push({
                label: node.label ?? '',
                indent: '\u00a0'.repeat(2 * (level - 1)),
                branch,
                leaf: !branch,
                expanded: expanded ? 'true' : 'false',
                id,
            });
            if (expanded) {
                walk(childrenOf(node), level + 1);
            }
        }
    };
    walk(el.nodes ?? [], 1);
    return out;
}

const branchIds = (nodes: TreeNode[], out: string[] = []): string[] => {
    for (const node of nodes) {
        const below = childrenOf(node);
        if (below.length > 0) {
            out.push(node.id ?? '');
            branchIds(below, out);
        }
    }
    return out;
};

/** A branch click toggles it; a leaf click commits `selected`. */
export function onRowClick(el: TreeHost, event: Event): void {
    const id = (event.currentTarget as HTMLElement | null)?.dataset.id ?? '';
    if (!id) {
        return;
    }
    if (!branchIds(el.nodes ?? []).includes(id)) {
        el.selected = id;
        return;
    }
    const expanded = el.expanded ?? [];
    el.expanded = expanded.includes(id)
        ? expanded.filter((entry) => entry !== id)
        : [...expanded, id];
}
