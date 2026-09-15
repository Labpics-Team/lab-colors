//! RED public WASM contract for FV-01.
//!
//! The browser seam must own an attached runtime and authority handles rather
//! than projecting detached ProgramSnapshot values as materialization proof.

use labcolors_wasm::{
    AttachedMaterializationAuthority, AttachedProgramRuntime, AttachedProgramUpdate,
    CompiledAttachedProgram, compile_attached_program_wire,
};
use wasm_bindgen::JsValue;

#[test]
fn wasm_exports_a_distinct_attached_program_owner_and_runtime() {
    let _ = compile_attached_program_wire as fn(&[u8]) -> Result<CompiledAttachedProgram, JsValue>;
    let _ = core::mem::size_of::<AttachedProgramRuntime>();
    let _ = core::mem::size_of::<AttachedProgramUpdate>();
    let _ = core::mem::size_of::<AttachedMaterializationAuthority>();
}
