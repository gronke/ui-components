// Tagged structured-clone JSON: Date, Map and Set survive the wire beside
// plain JSON, and the emitted text is canonical: object keys sort
// lexicographically at EVERY depth, so byte equality is state equality
// (ADR 0013's dedupe, extended below the top level; matching Rust needs
// its maps sorted too: serde_json's sort_all_objects).
//
// Numbers ride JSON.stringify, so exotic floats may differ from another
// serializer's spelling (1e21 → "1e+21" here); integer-bearing state is
// byte-stable everywhere.

const TAG = '$uic';
// A plain key that could be mistaken for the tag gains one leading '$' on
// encode and loses it on decode; the wrapper test stays a plain
// own-property check, no bottom-up reviver ambiguity.
const TAG_LIKE = /^\$+uic$/;
const ESCAPED_TAG = /^\$\$+uic$/;

export function encode(value: unknown): string {
    return serialize(value);
}

export function decode(text: string): unknown {
    return revive(JSON.parse(text));
}

function serialize(value: unknown): string {
    if (value === undefined) {
        return 'null';
    }
    if (value === null || typeof value !== 'object') {
        if (typeof value === 'function') {
            throw new Error('uic-sync codec: functions do not clone');
        }
        return JSON.stringify(value);
    }
    if (value instanceof Date) {
        return `{"${TAG}":"date","v":${JSON.stringify(value.toISOString())}}`;
    }
    if (value instanceof Map) {
        const entries = [...value.entries()].map(
            ([key, entry]) => `[${serialize(key)},${serialize(entry)}]`,
        );
        return `{"${TAG}":"map","v":[${entries.join(',')}]}`;
    }
    if (value instanceof Set) {
        const entries = [...value.values()].map((entry) => serialize(entry));
        return `{"${TAG}":"set","v":[${entries.join(',')}]}`;
    }
    if (Array.isArray(value)) {
        return `[${value.map((entry) => serialize(entry)).join(',')}]`;
    }
    const record = value as Record<string, unknown>;
    const fields = Object.keys(record)
        .filter((key) => record[key] !== undefined)
        .map((key) => [TAG_LIKE.test(key) ? '$' + key : key, record[key]] as const)
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([key, entry]) => `${JSON.stringify(key)}:${serialize(entry)}`);
    return `{${fields.join(',')}}`;
}

function revive(node: unknown): unknown {
    if (node === null || typeof node !== 'object') {
        return node;
    }
    if (Array.isArray(node)) {
        return node.map((entry) => revive(entry));
    }
    const record = node as Record<string, unknown>;
    if (Object.prototype.hasOwnProperty.call(record, TAG)) {
        const tag = record[TAG];
        if (tag === 'date') {
            return new Date(String(record.v));
        }
        if (tag === 'map') {
            const entries = record.v as [unknown, unknown][];
            return new Map(entries.map(([key, entry]) => [revive(key), revive(entry)]));
        }
        if (tag === 'set') {
            return new Set((record.v as unknown[]).map((entry) => revive(entry)));
        }
        throw new Error(`uic-sync codec: unknown tag ${JSON.stringify(tag)}`);
    }
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(record)) {
        out[ESCAPED_TAG.test(key) ? key.slice(1) : key] = revive(record[key]);
    }
    return out;
}
