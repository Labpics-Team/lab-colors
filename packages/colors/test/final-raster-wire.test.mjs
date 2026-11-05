import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  initSync, compileProgramWire, finalRasterSourceOverWire, isProgramError,
} from "@labpics/colors";
import { ProgramWireBuilderV1 } from "@labpics/colors/program-wire/abi-v1.js";
initSync({ module: readFileSync(new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm"))) });

const context = new Uint8Array(32).fill(7);
const opaque = (rgb) => Uint8Array.from(rgb.flatMap(pixel => [...pixel,255]));
function runtime(policy="mandatory-exact-nominal-srgb8-v2") { return compileProgramWire(finalRasterSourceOverWire(1n,2n,3n,17,policy),100); }
function update(rt, revision, source, destination, c=context, dpr=1) {
  return rt.updateFinalRaster(revision,2,2,dpr,c,1n,source,2n,destination);
}
function consume(rt, revision, consumer, c=context, content=rt.contentIdentity(), slot=17) {
  return rt.consumeFinalRaster(revision,content,slot,c,consumer);
}

test("public packaged field bytes are complete and synchronous consumer applies them", () => {
  const rt=runtime();
  const source=Uint8Array.from([64,64,64,128, 0,0,0,0, 32,32,32,255, 96,96,96,255]);
  const destination=opaque([[128,128,128],[64,64,64],[0,0,0],[0,0,0]]);
  const snapshot=update(rt,1n,source,destination);
  assert.equal(snapshot.state,"ready");
  assert.equal(snapshot.assessmentReport(),null);
  assert.equal(snapshot.outputCount(),0);
  const target=new Uint8Array(12);
  let calls=0;
  consume(rt,1n,frame => {
    assert.equal(frame.scope,"ModeledFinalRasterV1");
    assert.equal(frame.width*frame.height*3,frame.pixels.length);
    target.set(frame.pixels);
    calls++;
    assert.throws(() => rt.free(),error => error.code==="program_runtime_busy");
    return true;
  });
  assert.equal(calls,1);
  assert.deepEqual(target,Uint8Array.from([128,128,128,64,64,64,32,32,32,96,96,96]));
  snapshot.free();rt.free();
});

test("wrong point update is a typed Program error and preserves the current field", () => {
  const rt = runtime();
  const source = new Uint8Array(16);
  const destination = opaque(Array(4).fill([64,64,64]));
  let initial, retried, point, pointSnapshot;
  try {
    initial = update(rt,1n,source,destination);
    assert.equal(initial.state,"ready");
    assert.equal(initial.assessmentReport(),null);
    assert.equal(rt.finalRasterRefusal(),null);

    assert.throws(
      () => rt.updateObserved(2n,new Uint32Array([7]),new Uint8Array(),0),
      error => {
        assert.equal(isProgramError(error),true);
        assert.equal(error.code,"program_unsupported");
        assert.equal(error.operation,"updateObserved");
        return true;
      },
    );
    assert.equal(rt.finalRasterRefusal(),null);
    let calls = 0;
    consume(rt,1n,frame => {
      calls++;
      assert.deepEqual(frame.pixels,new Uint8Array(12).fill(64));
      return true;
    });
    assert.equal(calls,1);

    retried = update(rt,2n,source,opaque(Array(4).fill([65,65,65])));
    assert.equal(retried.state,"ready");
    assert.equal(retried.assessmentReport(),null);
    consume(rt,2n,frame => {
      calls++;
      assert.deepEqual(frame.pixels,new Uint8Array(12).fill(65));
      return true;
    });
    assert.equal(calls,2);

    const wire = new ProgramWireBuilderV1()
      .source(11,[64,64,64]).fixedTarget(21,11).surfaceInputPort(31)
      .solidPaint(41,21).inputSurface(51,31)
      .sourceOverOccurrence(61,41,51,64,0.2,1)
      .presentationRoot(71,61).presentationTarget(71,61)
      .exactVisibleUnary(true,81,61,[64,64,64]).output(91,41).finish();
    point = compileProgramWire(wire,101);
    pointSnapshot = point.updateObserved(1n,new Uint32Array([7]),new Uint8Array([0,0,0]),1);
    assert.equal(pointSnapshot.state,"ready");
    assert.deepEqual(pointSnapshot.outputRgb(0),new Uint8Array([64,64,64]));
  } finally {
    pointSnapshot?.free();
    point?.free();
    retried?.free();
    initial?.free();
    rt.free();
  }
});

test("last dirty final pixel commits Failed and never invokes consumer", () => {
  const rt=runtime();
  const clean=opaque(Array(4).fill([0,199,63]));
  const destination=opaque(Array(4).fill([0,0,0]));
  update(rt,1n,clean,destination).free();
  const dirty=clean.slice();dirty[14]=64;
  const snapshot=update(rt,2n,dirty,destination);
  assert.equal(snapshot.state,"failed");
  const refusal=rt.finalRasterRefusal();
  assert.equal(refusal.code,"program_field_convention");
  assert.equal(refusal.pixelIndex,3);
  assert.deepEqual(Array.from(refusal.rejectedBlueInterval),[64,108]);
  let calls=0;
  assert.throws(() => consume(rt,2n,() => {calls++;return true;}),isProgramError);
  assert.equal(calls,0);
  snapshot.free();rt.free();
});

test("complete shape, revision payload identity and every context binding are enforced", () => {
  const rt=runtime();
  const source=new Uint8Array(16);
  const destination=opaque(Array(4).fill([64,64,64]));
  update(rt,1n,source,destination).free();
  for(const altered of [source.slice(0,-4),new Uint8Array(20)]) {
    assert.throws(() => update(rt,2n,altered,destination),error => error.code==="program_field_length");
  }
  assert.throws(() => rt.updateFinalRaster(2n,65537,1,1,context,1n,new Uint8Array(),2n,new Uint8Array()),
    error => error.code==="program_resource_exhausted");
  const changed=destination.slice();changed[12]=65;changed[13]=65;changed[14]=65;
  assert.throws(() => update(rt,1n,source,changed),isProgramError);
  update(rt,1n,source,destination).free();
  update(rt,2n,source,changed,new Uint8Array(32).fill(8),2).free();
  let calls=0;
  const receiver=()=>{calls++;return true;};
  for(const invoke of [
    ()=>consume(rt,1n,receiver),
    ()=>consume(rt,2n,receiver),
    ()=>consume(rt,2n,receiver,new Uint8Array(32).fill(8),new Uint8Array(32)),
    ()=>consume(rt,2n,receiver,new Uint8Array(32).fill(8),rt.contentIdentity(),18),
  ]) assert.throws(invoke,isProgramError);
  assert.equal(calls,0);
  rt.updateUnknown(3n,55).free();
  assert.throws(()=>consume(rt,2n,receiver,new Uint8Array(32).fill(8)),isProgramError);
  update(rt,4n,source,destination).free();
  consume(rt,4n,receiver);
  assert.equal(calls,1);
  assert.throws(()=>consume(rt,4n,()=>false),error=>error.code==="program_field_consumer_rejected");
  rt.free();
});

test("field cleanliness is explicit and unsupported declarations fail", () => {
  for (const requirement of [undefined, "lint", "report-only", "auto", "future-profile"]) {
    assert.throws(() => finalRasterSourceOverWire(1n,2n,3n,17,requirement),
      error => isProgramError(error) && error.code === "program_field_unsupported");
  }
});

test("each guarded callback operation has a typed busy refusal and preserves current bytes", () => {
  const rt = runtime();
  const source = new Uint8Array(16);
  const destination = opaque(Array(4).fill([64,64,64]));
  update(rt,1n,source,destination).free();
  const identity = rt.contentIdentity();
  let nestedCalls = 0;
  let acceptedCalls = 0;
  rt.consumeFinalRaster(1n,identity,17,context,frame => {
    acceptedCalls++;
    const attempts = [
      ["updateObserved", () => rt.updateObserved(2n,new Uint32Array([7]),new Uint8Array([0,0,0]),1)],
      ["updateUnknown", () => rt.updateUnknown(2n,77)],
      ["updateFinalRaster", () => update(rt,2n,source,destination)],
      ["consumeFinalRaster", () => rt.consumeFinalRaster(1n,identity,17,context,() => { nestedCalls++; return true; })],
      ["contentIdentity", () => rt.contentIdentity()],
      ["finalRasterRefusal", () => rt.finalRasterRefusal()],
      ["free", () => rt.free()],
    ];
    if (Symbol.dispose) attempts.push(["free", () => rt[Symbol.dispose]()]);
    for (const [operation, invoke] of attempts) {
      assert.throws(invoke,error => isProgramError(error) &&
        error.code === "program_runtime_busy" && error.operation === operation);
    }
    assert.deepEqual(frame.pixels,Uint8Array.from(Array(4).fill([64,64,64]).flat()));
    return true;
  });
  assert.equal(acceptedCalls,1);
  assert.equal(nestedCalls,0);
  assert.deepEqual(rt.contentIdentity(),identity);
  assert.equal(rt.finalRasterRefusal(),null);
  rt.consumeFinalRaster(1n,identity,17,context,frame => {
    acceptedCalls++;
    assert.deepEqual(frame.pixels,Uint8Array.from(Array(4).fill([64,64,64]).flat()));
    return true;
  });
  assert.equal(acceptedCalls,2);
  rt.free();
});

test("runtime busy recognition accepts only actual guarded operations", () => {
  for (const operation of ["compileProgramWire", "finalRasterSourceOverWire", "assessmentReport",
    "attachmentFree", "contentIdentitiy", "unknownOperation"]) {
    const error = Object.assign(new Error("synthetic"), {operation, code:"program_runtime_busy"});
    assert.equal(isProgramError(error),false);
  }
});


test("off and lint preserve locked final bytes while mandatory refuses the same rejected field", () => {
  const source = new Uint8Array(16);
  const destination = opaque([[0,199,63],[0,199,64],[32,32,32],[0,199,64]]);
  const identities = new Set();
  for (const policy of ["off","lint-exact-nominal-srgb8-v2","mandatory-exact-nominal-srgb8-v2"]) {
    const rt=runtime(policy);
    const identity=Buffer.from(rt.contentIdentity()).toString("hex");
    assert.equal(identities.has(identity),false);identities.add(identity);
    const snapshot=update(rt,1n,source,destination);
    assert.equal(snapshot.assessmentReport(),null);
    if(policy === "mandatory-exact-nominal-srgb8-v2") {
      assert.equal(snapshot.state,"failed");
      assert.equal(rt.finalRasterRefusal().pixelIndex,1);
      let calls=0;
      assert.throws(()=>consume(rt,1n,()=>{calls++;return true;}),isProgramError);
      assert.equal(calls,0);
    } else {
      assert.equal(snapshot.state,"ready");assert.equal(rt.finalRasterRefusal(),null);
      consume(rt,1n,frame=>{
        assert.deepEqual(frame.pixels,Uint8Array.from([0,199,63,0,199,64,32,32,32,0,199,64]));
        assert.equal(Object.isFrozen(frame.cleanliness),true);
        if(policy === "off") {
          assert.deepEqual(frame.cleanliness,{mode:"off",verdict:"not-applied",profile:null,profileRelease:null,checkedPixelCount:0,violationCount:0,firstViolation:null});
        } else {
          const report=frame.cleanliness;
          assert.equal(report.mode,"lint");assert.equal(report.verdict,"violation");
          assert.equal(report.profile,"ExactNominalSrgb8CleanSetV2");assert.equal(report.profileRelease.length,32);
          assert.equal(report.checkedPixelCount,4);assert.equal(report.violationCount,2);
          assert.deepEqual(report.firstViolation,{pixelIndex:1,composite:[0,199,64],rejectedBlueInterval:[64,108]});
          assert.equal(Object.isFrozen(report.firstViolation),true);
        }
        return true;
      });
    }
    snapshot.free();rt.free();
  }
});

test("lint counts the last pixel, evaluates actual background, and cannot consume an old context", () => {
  const rt=runtime("lint-exact-nominal-srgb8-v2");
  const source = new Uint8Array(16);
  const background=opaque([[0,199,63],[0,199,63],[0,199,63],[0,199,64]]);
  update(rt,1n,source,background).free();
  let saved;
  consume(rt,1n,frame=>{saved=frame;return true;});
  assert.equal(saved.cleanliness.checkedPixelCount,4);
  assert.equal(saved.cleanliness.violationCount,1);
  assert.equal(saved.cleanliness.firstViolation.pixelIndex,3);
  const expectedRelease=saved.cleanliness.profileRelease.slice();
  saved.pixels.fill(255);saved.cleanliness.profileRelease.fill(255);
  consume(rt,1n,frame=>{
    assert.deepEqual(frame.cleanliness.profileRelease,expectedRelease);
    assert.deepEqual(frame.pixels,Uint8Array.from([0,199,63,0,199,63,0,199,63,0,199,64]));
    return true;
  });
  rt.updateUnknown(2n,55).free();
  let calls=0;
  assert.throws(()=>consume(rt,1n,()=>{calls++;return true;}),isProgramError);
  const changedContext=new Uint8Array(32).fill(8);
  const foreground=opaque(Array(4).fill([32,32,32]));
  update(rt,3n,foreground,background,changedContext).free();
  assert.throws(()=>consume(rt,3n,()=>{calls++;return true;}),isProgramError);
  consume(rt,3n,frame=>{
    assert.deepEqual(frame.pixels,new Uint8Array(12).fill(32));
    assert.equal(frame.cleanliness.verdict,"pass");
    assert.equal(frame.cleanliness.checkedPixelCount,4);
    assert.equal(frame.cleanliness.violationCount,0);
    assert.equal(frame.cleanliness.firstViolation,null);
    return true;
  },changedContext);
  assert.equal(calls,0);assert.equal(saved.cleanliness.verdict,"violation");
  rt.free();
});

test("off and lint still refuse missing pixels and transparent final results without invoking consumers", () => {
  for(const policy of ["off","lint-exact-nominal-srgb8-v2"]) {
    const rt=runtime(policy);
    const source=new Uint8Array(16);const background=opaque(Array(4).fill([32,32,32]));
    update(rt,1n,source,background).free();
    assert.throws(()=>update(rt,2n,source.slice(0,-4),background),e=>isProgramError(e)&&e.code==="program_field_length");
    consume(rt,1n,()=>true);
    const translucent=background.slice();translucent[15]=128;
    const failed=update(rt,2n,source,translucent);
    assert.equal(failed.state,"failed");assert.equal(rt.finalRasterRefusal().code,"program_field_transparent");
    assert.equal(rt.finalRasterRefusal().pixelIndex,3);
    let calls=0;assert.throws(()=>consume(rt,2n,()=>{calls++;return true;}),isProgramError);
    assert.equal(calls,0);failed.free();rt.free();
  }
});


test("field consumption uses captured copy and freeze intrinsics after module initialization", () => {
  const rt=runtime("lint-exact-nominal-srgb8-v2");
  const source=new Uint8Array(16);const destination=opaque(Array(4).fill([32,32,32]));
  update(rt,1n,source,destination).free();
  const originalFrom=Uint8Array.from;const originalFreeze=Object.freeze;
  let frame;
  try {
    Uint8Array.from=()=>{throw new Error("mutable global typed-array factory invoked");};
    Object.freeze=()=>{throw new Error("mutable global freeze invoked");};
    consume(rt,1n,value=>{frame=value;return true;});
  } finally { Uint8Array.from=originalFrom;Object.freeze=originalFreeze; }
  assert.deepEqual(frame.pixels,new Uint8Array(12).fill(32));
  assert.equal(Object.isFrozen(frame),true);assert.equal(Object.isFrozen(frame.cleanliness),true);
  assert.equal(frame.cleanliness.verdict,"pass");rt.free();
});
