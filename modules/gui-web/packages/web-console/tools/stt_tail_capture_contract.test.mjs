import assert from "node:assert/strict";

await import("../src/stt_tail_capture.js");

const api = globalThis.CoolzhuSttTailCapture;
const ring = new api.PcmRingBuffer(4);
ring.push(Int16Array.from([1, 2, 3]));
ring.push(Int16Array.from([4, 5, 6]));
assert.deepEqual(Array.from(ring.snapshot()), [3, 4, 5, 6]);

const wav = api.encodePcm16Wav(Int16Array.from([0, 32767, -32768]), 16000);
const view = new DataView(wav.buffer, wav.byteOffset, wav.byteLength);
assert.equal(new TextDecoder().decode(wav.slice(0, 4)), "RIFF");
assert.equal(new TextDecoder().decode(wav.slice(8, 12)), "WAVE");
assert.equal(view.getUint16(22, true), 1);
assert.equal(view.getUint32(24, true), 16000);
assert.equal(view.getUint16(34, true), 16);
assert.equal(view.getUint32(40, true), 6);
assert.equal(wav.byteLength, 50);

console.log("stt tail capture contracts: PASS");
