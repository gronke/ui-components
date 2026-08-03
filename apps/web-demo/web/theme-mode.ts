// The page's Bootstrap color mode: a stored choice wins, the OS scheme
// decides otherwise, and subscribers hear every change: the terminal pane
// re-derives its palette and repaints. The template's inline head script
// applies the same rule before first paint.

type Theme = 'light' | 'dark';

const KEY = 'uic-theme';
const media = matchMedia('(prefers-color-scheme: dark)');
const listeners: ((theme: Theme) => void)[] = [];

function storedTheme(): Theme | null {
    const stored = localStorage.getItem(KEY);
    return stored === 'light' || stored === 'dark' ? stored : null;
}

export function effectiveTheme(): Theme {
    return storedTheme() ?? (media.matches ? 'dark' : 'light');
}

export function applyTheme(theme: Theme): void {
    document.documentElement.setAttribute('data-bs-theme', theme);
    for (const listener of listeners) {
        listener(theme);
    }
}

export function onThemeChange(listener: (theme: Theme) => void): void {
    listeners.push(listener);
}

/** Wires the two-state toggle and the OS-scheme watcher. */
export function wireThemeToggle(button: HTMLElement): void {
    const label = () => {
        const dark = effectiveTheme() === 'dark';
        button.textContent = dark ? '☀' : '☾';
        button.title = dark ? 'Switch to the light theme' : 'Switch to the dark theme';
    };
    button.addEventListener('click', () => {
        const next: Theme = effectiveTheme() === 'dark' ? 'light' : 'dark';
        localStorage.setItem(KEY, next);
        applyTheme(next);
        label();
    });
    // The OS scheme steers only while no explicit choice is stored.
    media.addEventListener('change', () => {
        if (!storedTheme()) {
            applyTheme(effectiveTheme());
            label();
        }
    });
    applyTheme(effectiveTheme());
    label();
}
