// One wiring for every pane showing the state: seed the dedupe with the
// settled boot state (booting is not news), send commits that are news,
// apply the peers' snapshots, and record both directions for the debug
// bar.
import { StateBridge } from './bridge.js';
import type { AppState } from './bridge.js';

export type PaneEntry = { src: string; type: string; state: AppState };

export function wireStatePane(options: {
  boot: AppState;
  apply: (state: AppState) => void;
  src: string;
  record: (entry: PaneEntry) => void;
  bridge?: StateBridge;
}): { changed: (state: AppState) => void } {
  const bridge = options.bridge ?? new StateBridge();
  bridge.remember(options.boot);
  bridge.onState((state) => {
    options.apply(state);
    options.record({ src: options.src, type: 'state-applied', state });
  });
  return {
    changed: (state) => {
      if (bridge.send(state)) {
        options.record({ src: options.src, type: 'state-changed', state });
      }
    },
  };
}
