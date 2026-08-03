// The camera QR scan: a BarcodeDetector loop over a getUserMedia stream.
// Browser-only by nature; the caller checks `'BarcodeDetector' in window`
// before offering the button.
import { linkPayload, payloadRole } from '../@gronke/uic-sync/pair.js';

/** The corner of the BarcodeDetector API the scan uses — the platform type
 * is not in TS's DOM lib yet. */
interface DetectedCode {
    rawValue: string;
}

declare global {
    interface Window {
        BarcodeDetector?: new (options: { formats: string[] }) => {
            detect(source: HTMLVideoElement): Promise<DetectedCode[]>;
        };
    }
}

/** Runs the camera against QR codes until one carries a PEER's swap
 * payload — this side's own code (or stray QR noise) keeps the camera
 * looking. Resolves with the raw scanned text; the stream always stops. */
export async function scanFor(video: HTMLVideoElement, own: string): Promise<string> {
    const detector = new window.BarcodeDetector!({ formats: ['qr_code'] });
    const stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: 'environment' },
    });
    video.srcObject = stream;
    await video.play();
    try {
        for (;;) {
            const codes = await detector.detect(video);
            const hit = codes.find((code) => {
                const payload = linkPayload(String(code.rawValue));
                return payloadRole(payload) === 'offer' && payload !== own;
            });
            if (hit) {
                return String(hit.rawValue);
            }
            await new Promise((resolve) => setTimeout(resolve, 250));
        }
    } finally {
        stream.getTracks().forEach((track) => track.stop());
        video.srcObject = null;
    }
}
