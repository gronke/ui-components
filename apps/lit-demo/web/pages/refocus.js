// Typing always lands in the list: a click on page chrome refocuses the
// active input: the row being edited, or the entry row. Shared by every
// page that mounts the app; the selector names every control a page may
// carry, and the extras are harmless where they never match. A hidden app
// (the p2p page before pairing) keeps the keyboard where it is.
document.addEventListener('pointerdown', (event) => {
    if (!event.target.closest('a, button, input, textarea, summary, video')) {
        queueMicrotask(() => {
            const app = document.querySelector('todo-app');
            if (!app || app.closest('[hidden]')) {
                return;
            }
            (app.querySelector('todo-item input.label') ?? app.querySelector('input.draft'))?.focus();
        });
    }
});
