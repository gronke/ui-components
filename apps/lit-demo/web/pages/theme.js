// The theme boot, shared by every page: a classic parser-blocking script,
// so the attribute lands before the first paint — no light flash on a dark
// system. The choice persists in localStorage under 'uic-theme'.
document.documentElement.setAttribute(
    'data-bs-theme',
    localStorage.getItem('uic-theme') ??
        (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'),
);
