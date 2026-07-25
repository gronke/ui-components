// The one wire seam (ADR 0024): string payloads over whatever carries them.
// Demo-grade by design — a dead wire stays dead (no reconnect), one
// listener per event, sends before open drop silently.

export interface Wire {
    send(text: string): void;
    onMessage(callback: (text: string) => void): void;
    onOpen(callback: () => void): void;
    /** Fires once when the wire goes away — the far side closed, the
     * transport tore down, or close() was called here. */
    onClose(callback: () => void): void;
    close(): void;
}

export class WebSocketWire implements Wire {
    socket: WebSocket;

    constructor(target: string | URL | WebSocket) {
        this.socket = target instanceof WebSocket ? target : new WebSocket(target);
    }

    send(text: string): void {
        if (this.socket.readyState === WebSocket.OPEN) {
            this.socket.send(text);
        }
    }

    onMessage(callback: (text: string) => void): void {
        this.socket.addEventListener('message', (event) => callback(String(event.data)));
    }

    onOpen(callback: () => void): void {
        if (this.socket.readyState === WebSocket.OPEN) {
            queueMicrotask(callback);
        } else {
            this.socket.addEventListener('open', () => callback(), { once: true });
        }
    }

    onClose(callback: () => void): void {
        this.socket.addEventListener('close', () => callback(), { once: true });
    }

    close(): void {
        this.socket.close();
    }
}

export class DataChannelWire implements Wire {
    channel: RTCDataChannel;

    constructor(channel: RTCDataChannel) {
        this.channel = channel;
    }

    send(text: string): void {
        if (this.channel.readyState === 'open') {
            this.channel.send(text);
        }
    }

    onMessage(callback: (text: string) => void): void {
        this.channel.addEventListener('message', (event) => callback(String(event.data)));
    }

    onOpen(callback: () => void): void {
        if (this.channel.readyState === 'open') {
            queueMicrotask(callback);
        } else {
            this.channel.addEventListener('open', () => callback(), { once: true });
        }
    }

    onClose(callback: () => void): void {
        this.channel.addEventListener('close', () => callback(), { once: true });
    }

    close(): void {
        this.channel.close();
    }
}

export class BroadcastWire implements Wire {
    channel: BroadcastChannel;
    closed: (() => void) | null = null;

    constructor(name: string) {
        this.channel = new BroadcastChannel(name);
    }

    send(text: string): void {
        this.channel.postMessage(text);
    }

    onMessage(callback: (text: string) => void): void {
        this.channel.onmessage = (event) => callback(String(event.data));
    }

    onOpen(callback: () => void): void {
        queueMicrotask(callback);
    }

    // BroadcastChannel has no close event — only the local close() reports.
    onClose(callback: () => void): void {
        this.closed = callback;
    }

    close(): void {
        this.channel.close();
        this.closed?.();
    }
}
