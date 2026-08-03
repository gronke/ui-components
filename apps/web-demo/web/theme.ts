// The terminal palette from Bootstrap's own variables: the runtime speaks
// plain ANSI colors (a real terminal keeps the user's scheme), and this
// maps those slots to the stylesheet's custom properties AS RESOLVED ON THE
// SCREEN ELEMENT: both theme variants are always on offer, and the screen
// picks one by wearing data-bs-theme (dark here, while the site wears
// light; flip either and its colors follow).
export function bootstrapTheme(screen: HTMLElement): Record<string, string> {
  const bs = (name: string) => getComputedStyle(screen).getPropertyValue(name).trim();
  return {
    background: bs('--bs-body-bg'),
    foreground: bs('--bs-body-color'),
    cursor: bs('--bs-body-color'),
    red: bs('--bs-danger'),
    green: bs('--bs-success'),
    yellow: bs('--bs-warning'),
    blue: bs('--bs-primary'),
    cyan: bs('--bs-info'),
    brightBlack: bs('--bs-secondary'),
    brightRed: bs('--bs-danger-text-emphasis'),
    brightGreen: bs('--bs-success-text-emphasis'),
    brightYellow: bs('--bs-warning-text-emphasis'),
    brightBlue: bs('--bs-primary-text-emphasis'),
    brightCyan: bs('--bs-info-text-emphasis'),
  };
}
