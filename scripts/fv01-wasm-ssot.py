from pathlib import Path

lib_path = Path("crates/labcolors-wasm/src/lib.rs")
lib = lib_path.read_text()
anchor = "fn checked_output_index(value: JsValue) -> Result<usize, JsError> {"
helper = '''fn project_program_scenarios(
    scenario_ids: &[u32],
    surfaces: &[u8],
    surface_count: usize,
) -> Result<Vec<labcolors_core::program_wire::ProgramScenarioV1>, ()> {
    let row_bytes = surface_count.checked_mul(3).ok_or(())?;
    let expected = scenario_ids.len().checked_mul(row_bytes).ok_or(())?;
    if expected != surfaces.len() {
        return Err(());
    }

    let mut scenarios = Vec::new();
    scenarios
        .try_reserve_exact(scenario_ids.len())
        .map_err(|_| ())?;
    for (row, scenario_id) in scenario_ids.iter().copied().enumerate() {
        let start = row.checked_mul(row_bytes).ok_or(())?;
        let mut values = Vec::new();
        values.try_reserve_exact(surface_count).map_err(|_| ())?;
        for offset in 0..surface_count {
            let byte = start
                .checked_add(offset.checked_mul(3).ok_or(())?)
                .ok_or(())?;
            values.push(labcolors_core::Srgb8::new([
                surfaces[byte],
                surfaces[byte + 1],
                surfaces[byte + 2],
            ]));
        }
        scenarios.push(labcolors_core::program_wire::ProgramScenarioV1::new(
            scenario_id,
            values,
        ));
    }
    Ok(scenarios)
}

'''
if lib.count(anchor) != 1 or "fn project_program_scenarios(" in lib:
    raise SystemExit("scenario helper anchor changed")
lib = lib.replace(anchor, helper + anchor, 1)

start = lib.find("        let row_bytes = surface_count\n", lib.find("impl ProgramRuntime"))
end_marker = "        self.inner\n            .update_observed(revision, &scenarios)"
end = lib.find(end_marker, start)
if start < 0 or end < 0:
    raise SystemExit("detached scenario parser anchors changed")
replacement = '''        let scenarios = project_program_scenarios(scenario_ids, surfaces, surface_count)
            .map_err(|_| to_program_js_error(E::Update, ProgramOperation::UpdateObserved))?;
'''
lib = lib[:start] + replacement + lib[end:]
lib_path.write_text(lib)

path = Path("crates/labcolors-wasm/src/attached.rs")
text = path.read_text()
old_import = "use js_sys::{Array, Function, Object, Reflect, Uint8Array};"
if text.count(old_import) != 1:
    raise SystemExit("js-sys import anchor changed")
text = text.replace(
    old_import,
    "use js_sys::{Array, Function, Object, Reflect, Uint8Array, Uint32Array};",
    1,
)
text = text.replace(
    "    ProgramScenarioV1, RendererProvenanceV1, compile_attached_program_wire_v1,\n",
    "    RendererProvenanceV1, compile_attached_program_wire_v1,\n",
    1,
)

ts_anchor = '''/** Synchronous host transaction boundary. The implementation lives in the consumer harness. */
export interface AttachedPointSinkHostV1 {
  tryInstall(intent: AttachedPointSinkHostIntentV1): void;
}
'''
ts_new = '''/** Exact authored bindings for one attachment. */
export interface AttachedProgramBindingsV1 {
  readonly emissionOutputs: Uint32Array;
  readonly emissionSinkOutputs: Uint32Array;
  readonly presentationOutputs: Uint32Array;
  readonly presentationRoots: Uint32Array;
  readonly presentationOccurrences: Uint32Array;
}

/** Synchronous host transaction boundary. The implementation lives in the consumer harness. */
export interface AttachedPointSinkHostV1 {
  tryInstall(intent: AttachedPointSinkHostIntentV1): void;
}
'''
if text.count(ts_anchor) != 1:
    raise SystemExit("TS host anchor changed")
text = text.replace(ts_anchor, ts_new, 1)

parser_start = text.find("pub(super) fn project_program_scenarios(")
parser_end = text.find("\n#[wasm_bindgen]\npub struct CompiledAttachedProgram", parser_start)
if parser_start < 0 or parser_end < 0:
    raise SystemExit("attached scenario parser anchors changed")
text = text[:parser_start] + text[parser_end + 1:]

helper_anchor = "#[wasm_bindgen]\nimpl CompiledAttachedProgram {\n"
binding_helper = '''fn u32_array_property(bindings: &JsValue, key: &str) -> Result<Vec<u32>, JsValue> {
    let value = Reflect::get(bindings, &JsValue::from_str(key))?;
    let array = value
        .dyn_into::<Uint32Array>()
        .map_err(|_| attached_error("attached_invalid_bindings", OP_ATTACH))?;
    Ok(array.to_vec())
}

'''
if text.count(helper_anchor) != 1 or "fn u32_array_property(" in text:
    raise SystemExit("compiled attached anchor changed")
text = text.replace(helper_anchor, binding_helper + helper_anchor, 1)

attach_start = text.find("    #[wasm_bindgen(js_name = attach)]\n", text.find("impl CompiledAttachedProgram"))
attach_end = text.find("\n}\n\n#[wasm_bindgen]\npub struct AttachedProgramRuntime", attach_start)
if attach_start < 0 or attach_end < 0:
    raise SystemExit("wide attach anchors changed")
new_attach = '''    #[wasm_bindgen(js_name = attach)]
    pub fn attach(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] stream_id: JsValue,
        #[wasm_bindgen(unchecked_param_type = "AttachedProgramBindingsV1")] bindings: JsValue,
        #[wasm_bindgen(unchecked_param_type = "AttachedPointSinkHostV1")] host: JsValue,
    ) -> Result<AttachedProgramRuntime, JsValue> {
        let stream_id = super::checked_u32(stream_id)
            .ok_or_else(|| attached_error("attached_instantiate", OP_ATTACH))?;
        let emission_outputs = u32_array_property(&bindings, "emissionOutputs")?;
        let emission_sink_outputs = u32_array_property(&bindings, "emissionSinkOutputs")?;
        let presentation_outputs = u32_array_property(&bindings, "presentationOutputs")?;
        let presentation_roots = u32_array_property(&bindings, "presentationRoots")?;
        let presentation_occurrences = u32_array_property(&bindings, "presentationOccurrences")?;
        if emission_outputs.len() != emission_sink_outputs.len()
            || presentation_outputs.len() != presentation_roots.len()
            || presentation_outputs.len() != presentation_occurrences.len()
        {
            return Err(attached_error("attached_invalid_bindings", OP_ATTACH));
        }

        let mut emissions = Vec::new();
        emissions
            .try_reserve_exact(emission_outputs.len())
            .map_err(|_| attached_error("attached_resource_exhausted", OP_ATTACH))?;
        emissions.extend(
            emission_outputs
                .iter()
                .copied()
                .zip(emission_sink_outputs.iter().copied())
                .map(|(output, sink_output)| {
                    AttachedProgramEmissionBindingV1::new(output, sink_output)
                }),
        );

        let mut presentations = Vec::new();
        presentations
            .try_reserve_exact(presentation_outputs.len())
            .map_err(|_| attached_error("attached_resource_exhausted", OP_ATTACH))?;
        presentations.extend(
            presentation_outputs
                .iter()
                .copied()
                .zip(presentation_roots.iter().copied())
                .zip(presentation_occurrences.iter().copied())
                .map(|((output, root), occurrence)| {
                    AttachedProgramPresentationBindingV1::new(output, root, occurrence)
                }),
        );

        let surface_input_count = self.inner.surface_input_count();
        self.inner
            .attach(
                stream_id,
                &emissions,
                &presentations,
                JsPointSinkHost { target: host },
            )
            .map(|inner| AttachedProgramRuntime {
                inner,
                surface_input_count,
            })
            .map_err(map_attach_error)
    }
'''
text = text[:attach_start] + new_attach + text[attach_end:]
old_call = "let scenarios = project_program_scenarios(scenario_ids, surfaces, self.surface_input_count)"
if text.count(old_call) != 1:
    raise SystemExit("attached scenario call anchor changed")
text = text.replace(
    old_call,
    "let scenarios = super::project_program_scenarios(scenario_ids, surfaces, self.surface_input_count)",
    1,
)
path.write_text(text)
