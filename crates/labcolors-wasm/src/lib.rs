//! `@labpics/colors` — терминальная WASM-граница C7c.
//!
//! Один runtime-root: canonical Program wire → ProgramRuntime → ProgramSnapshot.
//! Legacy recipe engine (`LabColors.resolveTheme/loadConfig`) удалён. DOM/CSS
//! effects принадлежат npm-приложению, не WASM.

mod error;
mod terminal_projection;

use wasm_bindgen::prelude::*;

use crate::error::BindingError;

fn to_js_error(error: BindingError) -> JsError {
    JsError::new(&error.to_string())
}

/// Единственный публичный манифест численных возможностей.
#[wasm_bindgen(js_name = numericalCapabilityManifest)]
pub fn numerical_capability_manifest() -> Result<JsValue, JsError> {
    let json = terminal_projection::capability_manifest_json();
    js_sys::JSON::parse(&json).map_err(|_| {
        to_js_error(BindingError::Internal {
            reason: "capability manifest не распарсился как JSON".to_string(),
        })
    })
}

/// Точная оценка WCAG 2.2 одной финальной пары sRGB8.
#[wasm_bindgen(js_name = evaluateWcag22)]
pub fn evaluate_wcag22(
    foreground_hex: &str,
    background_hex: &str,
    criterion: &str,
) -> Result<JsValue, JsError> {
    use labcolors_core::wcag22::Wcag22CriterionV1 as C;
    let criterion = C::parse(criterion).ok_or_else(|| {
        to_js_error(BindingError::UnknownWcag22Criterion {
            requested: criterion.to_string(),
        })
    })?;
    let assessment =
        labcolors_core::wcag22::evaluate_wcag22_hex(foreground_hex, background_hex, criterion)
            .map_err(|error| {
                use labcolors_core::wcag22::Wcag22EvaluationErrorV1 as E;
                to_js_error(match error {
                    E::InvalidSrgb8 { field, reason } => BindingError::InvalidColor {
                        reason: format!("{field}: {reason}"),
                    },
                    other => BindingError::Internal {
                        reason: other.to_string(),
                    },
                })
            })?;
    let json = terminal_projection::wcag22_json(&assessment).map_err(to_js_error)?;
    js_sys::JSON::parse(&json).map_err(|_| {
        to_js_error(BindingError::Internal {
            reason: "WCAG22 projection не распарсился как JSON".to_string(),
        })
    })
}

/// Публичный runtime одного скомпилированного Program.
#[wasm_bindgen]
pub struct ProgramRuntime {
    inner: labcolors_core::program_wire::ProgramSessionV1,
}

/// Owned snapshot одного Program update.
#[wasm_bindgen]
pub struct ProgramSnapshot {
    inner: labcolors_core::program_wire::ProgramSnapshotV1,
}

#[wasm_bindgen]
impl ProgramSnapshot {
    /// Stable lifecycle key: waiting|ready|stale|failed.
    #[wasm_bindgen(getter)]
    pub fn state(&self) -> String {
        use labcolors_core::program_wire::ProgramSnapshotStateV1 as S;
        match self.inner.state() {
            S::Waiting => "waiting",
            S::Ready => "ready",
            S::Stale => "stale",
            S::Failed => "failed",
            _ => "unknown",
        }
        .to_string()
    }

    #[wasm_bindgen(js_name = outputCount)]
    pub fn output_count(&self) -> usize {
        self.inner.outputs().len()
    }

    #[wasm_bindgen(js_name = outputSlot)]
    pub fn output_slot(&self, index: usize) -> Result<u32, JsError> {
        self.inner
            .outputs()
            .get(index)
            .map(|output| output.slot())
            .ok_or_else(|| {
                to_js_error(BindingError::Internal {
                    reason: format!("program output index {index} is out of bounds"),
                })
            })
    }

    #[wasm_bindgen(js_name = outputRgb)]
    pub fn output_rgb(&self, index: usize) -> Result<Box<[u8]>, JsError> {
        self.inner
            .outputs()
            .get(index)
            .map(|output| output.source().bytes().to_vec().into_boxed_slice())
            .ok_or_else(|| {
                to_js_error(BindingError::Internal {
                    reason: format!("program output index {index} is out of bounds"),
                })
            })
    }

    #[wasm_bindgen(js_name = outputOpacity)]
    pub fn output_opacity(&self, index: usize) -> Result<f64, JsError> {
        self.inner
            .outputs()
            .get(index)
            .map(|output| output.opacity())
            .ok_or_else(|| {
                to_js_error(BindingError::Internal {
                    reason: format!("program output index {index} is out of bounds"),
                })
            })
    }
}

/// Компилирует canonical Program wire bytes и создаёт одну runtime Session.
#[wasm_bindgen(js_name = compileProgramWire)]
pub fn compile_program_wire(bytes: &[u8], stream_id: u32) -> Result<ProgramRuntime, JsError> {
    let compiled = labcolors_core::program_wire::compile_program_wire_v1(bytes).map_err(|_| {
        to_js_error(BindingError::InvalidConfig {
            reason: "program wire was rejected".to_string(),
        })
    })?;
    let session = compiled.instantiate(stream_id).map_err(|_| {
        to_js_error(BindingError::InvalidConfig {
            reason: "program runtime instantiation was rejected".to_string(),
        })
    })?;
    Ok(ProgramRuntime { inner: session })
}

#[wasm_bindgen]
impl ProgramRuntime {
    /// Атомарный observed update. `surfaces` — row-major
    /// `scenario_ids.len() × surface_count × 3` bytes.
    #[wasm_bindgen(js_name = updateObserved)]
    pub fn update_observed(
        &mut self,
        revision: u64,
        scenario_ids: &[u32],
        surfaces: &[u8],
        surface_count: usize,
    ) -> Result<ProgramSnapshot, JsError> {
        let row_bytes = surface_count.checked_mul(3).ok_or_else(|| {
            to_js_error(BindingError::InvalidConfig {
                reason: "program surface row length overflow".to_string(),
            })
        })?;
        let expected = scenario_ids.len().checked_mul(row_bytes).ok_or_else(|| {
            to_js_error(BindingError::InvalidConfig {
                reason: "program scenario matrix length overflow".to_string(),
            })
        })?;
        if expected != surfaces.len() {
            return Err(to_js_error(BindingError::InvalidConfig {
                reason: format!(
                    "program scenario matrix length mismatch: expected {expected}, got {}",
                    surfaces.len()
                ),
            }));
        }
        let mut scenarios = Vec::new();
        scenarios
            .try_reserve_exact(scenario_ids.len())
            .map_err(|_| {
                to_js_error(BindingError::Internal {
                    reason: "program scenario allocation failed".to_string(),
                })
            })?;
        for (row, scenario_id) in scenario_ids.iter().copied().enumerate() {
            let start = row * row_bytes;
            let mut values = Vec::new();
            values.try_reserve_exact(surface_count).map_err(|_| {
                to_js_error(BindingError::Internal {
                    reason: "program surface allocation failed".to_string(),
                })
            })?;
            for offset in 0..surface_count {
                let byte = start + offset * 3;
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
        self.inner
            .update_observed(revision, &scenarios)
            .map(|inner| ProgramSnapshot { inner })
            .map_err(|_| {
                to_js_error(BindingError::InvalidConfig {
                    reason: "program update was rejected".to_string(),
                })
            })
    }

    #[wasm_bindgen(js_name = updateUnknown)]
    pub fn update_unknown(
        &mut self,
        revision: u64,
        reason_id: u32,
    ) -> Result<ProgramSnapshot, JsError> {
        self.inner
            .update_unknown(revision, reason_id)
            .map(|inner| ProgramSnapshot { inner })
            .map_err(|_| {
                to_js_error(BindingError::InvalidConfig {
                    reason: "program unknown update was rejected".to_string(),
                })
            })
    }
}
