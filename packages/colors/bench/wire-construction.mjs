// Compare the public producer, independently of WASM compilation and execution.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const [mode, left, right] = process.argv.slice(2);
const sizes = [1, 128, 4096];
async function measure(root) {
  const results = [];
  for (const version of [1, 2]) {
    const abi = await import(pathToFileURL(resolve(root, `program-wire/abi-v.js`)));
    const Builder = abi[`ProgramWireBuilderV`];
    for (const count of sizes) {
      const samples = [];
      for (let sample = -3; sample < 7; sample++) {
        const start = process.hrtime.bigint();
        const b = new Builder().surfaceInputPort(1).inputSurface(1, 1);
        for (let id = 1; id <= count; id++) {
          const rgb = [id & 255, (id >>> 8) & 255, (id + sample) & 255];
          if (version === 2 && id === 1) {
            b.finiteTarget(id, [{ id: 1, rgb, opacity: 1 }, { id: 2, rgb: [33, 23, 13], opacity: 0.5 }]);
          } else {
            b.source(id, rgb).fixedTarget(id, id);
          }
          b.solidPaint(id, id).sourceOverOccurrence(id, id, 1, 64, 0.2, abi.SURROUND_AVERAGE_V1)
            .presentationRoot(id, id).presentationTarget(id, id)
            .wcag22VisibleUnary(false, id, id, abi.WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1).output(id, id);
        }
        if (version === 2) b.selectionRelease(1n, [[{ id: 1, choices: [{ target: 1, candidate: 1 }] }],
          [{ id: 2, choices: [{ target: 1, candidate: 2 }] }]]);
        const built = process.hrtime.bigint();
        const wire = b.finish();
        const done = process.hrtime.bigint();
        const header = new DataView(wire.buffer, wire.byteOffset, wire.byteLength);
        assert.equal(header.getUint16(4, true), version);
        assert.equal(header.getUint32(6, true), wire.length);
        const sha256 = createHash("sha256").update(wire).digest("hex");
        if (sample >= 0) samples.push({ buildNs: Number(built - start), finishNs: Number(done - built),
          totalNs: Number(done - start), bytes: wire.length, sha256 });
      }
      results.push({ version, count, samples });
    }
  }
  return results;
}

if (mode === "measure" && left && !right) {
  console.log(JSON.stringify(await measure(left)));
} else if (mode === "compare" && left && right) {
  const roots = { baseline: left, candidate: right };
  const med = (v) => {
    const s = v.toSorted((a, b) => a - b);
    return (s[Math.floor((s.length - 1) / 2)] + s[Math.floor(s.length / 2)]) / 2;
  };
  const records = { baseline: [], candidate: [] };
  for (let block = 0; block < 4; block++) {
    const order = block % 2 ? ["candidate", "baseline"] : ["baseline", "candidate"];
    for (const label of order) {
      const raw = execFileSync(process.execPath, [fileURLToPath(import.meta.url), "measure", roots[label]],
        { encoding: "utf8", timeout: 90000 });
      const rows = JSON.parse(raw);
      assert.deepEqual(rows.map(({ version, count }) => [version, count]), [1, 2].flatMap(v => sizes.map(n => [v, n])));
      assert.ok(rows.every(r => r.samples.length === 7 && r.samples.every(s =>
        s.buildNs > 0 && s.finishNs > 0 && s.totalNs === s.buildNs + s.finishNs)));
      records[label].push(rows);
      console.log(JSON.stringify({ wireProducerSamples: { label, block, rows } }));
    }
    const subjects = rows => rows.map(r => r.samples.map(s => [s.bytes, s.sha256]));
    assert.deepEqual(subjects(records.baseline[block]), subjects(records.candidate[block]), "authored wire changed");
  }
  for (const [i, { version, count }] of records.baseline[0].entries()) {
    const result = { version, count };
    for (const label of Object.keys(roots)) {
      result[label] = Object.fromEntries(["buildNs", "finishNs", "totalNs"].map(k => [k,
        records[label].map(rows => med(rows[i].samples.map(s => s[k])))]));
    }
    console.log(JSON.stringify({ wireProducerSummary: result }));
  }
} else {
  throw new Error("usage: node wire-construction.mjs compare BASE_PACKAGE CANDIDATE_PACKAGE");
}
