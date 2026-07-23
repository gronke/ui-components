// The WebRTC showcase: two browsers pair through mutually shown QR codes —
// the offer rides this page's URL fragment (a phone camera opens it), the
// answer travels back by scan or paste — and the same <todo-app> syncs
// peer-to-peer over the data channel. No server carries state or signaling.

import { attach } from '../@schuhkarton/uic-sync/sync.js';
import { createHost, join } from '../@schuhkarton/uic-sync/pair.js';
import type { Wire } from '../@schuhkarton/uic-sync/wire.js';
import qrcode from 'qrcode-generator';

const FIELDS = ['draft', 'editing', 'items', 'selected'];

function element(id: string): HTMLElement {
    const found = document.getElementById(id);
    if (!found) {
        throw new Error(`p2p page: no #${id}`);
    }
    return found;
}

function status(text: string): void {
    element('status').textContent = text;
}

function renderQr(target: HTMLElement, text: string): void {
    const code = qrcode(0, 'L');
    code.addData(text);
    code.make();
    target.innerHTML = code.createSvgTag({ cellSize: 4, margin: 4 });
}

/** The scanned or pasted text, reduced to the compact payload — a full
 * page link carries it behind '#o='. */
function payloadOf(text: string): string {
    const trimmed = text.trim();
    const marker = trimmed.indexOf('#o=');
    return marker >= 0 ? decodeURIComponent(trimmed.slice(marker + 3)) : trimmed;
}

async function connectApp(wire: Wire, greet: boolean): Promise<void> {
    await customElements.whenDefined('todo-app');
    const el = document.querySelector('todo-app');
    if (el) {
        attach(el, { fields: FIELDS, wire, greet });
    }
    status(greet ? 'Connected — this list is the shared one now.' : 'Connected — editing the host’s list.');
}

async function scanAnswer(): Promise<string> {
    const detector = new (window as any).BarcodeDetector({ formats: ['qr_code'] });
    const video = element('camera') as HTMLVideoElement;
    const stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: 'environment' },
    });
    video.srcObject = stream;
    video.hidden = false;
    await video.play();
    try {
        for (;;) {
            const codes = await detector.detect(video);
            const hit = codes.find((code: any) => String(code.rawValue).includes('uics1.'));
            if (hit) {
                return String(hit.rawValue);
            }
            await new Promise((resolve) => setTimeout(resolve, 250));
        }
    } finally {
        stream.getTracks().forEach((track) => track.stop());
        video.hidden = true;
    }
}

async function hostFlow(): Promise<void> {
    element('share').hidden = true;
    status('Gathering candidates…');
    const host = await createHost();
    const link = new URL(location.pathname + location.search, location.href);
    link.hash = '#o=' + encodeURIComponent(host.offer);
    renderQr(element('offer-qr'), link.href);
    (element('offer-text') as HTMLTextAreaElement).value = host.offer;
    element('offer-block').hidden = false;
    status('Waiting for the answer…');

    if ('BarcodeDetector' in window && navigator.mediaDevices) {
        const scan = element('scan');
        scan.hidden = false;
        scan.addEventListener('click', async () => {
            try {
                status('Point the camera at the answer code…');
                (element('answer-input') as HTMLTextAreaElement).value = await scanAnswer();
                status('Answer scanned — connect when ready.');
            } catch (error) {
                status(`Camera unavailable (${error}) — paste the answer instead.`);
            }
        });
    }

    element('connect').addEventListener('click', async () => {
        const text = (element('answer-input') as HTMLTextAreaElement).value;
        if (!text.trim()) {
            status('Paste (or scan) the answer first.');
            return;
        }
        try {
            status('Connecting…');
            const wire = await host.complete(payloadOf(text));
            element('offer-block').hidden = true;
            await connectApp(wire, true);
        } catch (error) {
            status(`Pairing failed: ${error}`);
        }
    });
}

async function guestFlow(offer: string): Promise<void> {
    element('host-controls').hidden = true;
    status('Answering the offer…');
    const guest = await join(offer);
    renderQr(element('answer-qr'), guest.answer);
    (element('answer-text') as HTMLTextAreaElement).value = guest.answer;
    element('guest-block').hidden = false;
    status('Show the answer to the host and wait…');
    const wire = await guest.wire;
    element('guest-block').hidden = true;
    await connectApp(wire, false);
}

function boot(): void {
    if (location.hash.startsWith('#o=')) {
        guestFlow(payloadOf(location.hash)).catch((error) => status(`Pairing failed: ${error}`));
        return;
    }
    if (['localhost', '127.0.0.1'].includes(location.hostname)) {
        status('Open this page through your LAN address so the QR reaches other devices.');
    }
    element('share').addEventListener('click', () => {
        hostFlow().catch((error) => status(`Pairing failed: ${error}`));
    });
}

boot();
