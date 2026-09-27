//! Общий точечный вход для независимых проверок TQ и CC.
use crate::Srgb8;
use crate::program::wire::ProgramWireBuilderV1;
use crate::program_wire::{
    ProgramAttachmentV1, ProgramPointSinkHostV1, ProgramPointSinkHostErrorV1,
    ProgramPointSinkIntentV1, compile_program_wire_v1,
};

const OUTPUT: u32 = 17;
const ROOT: u32 = 9;
const OCCURRENCE: u32 = 8;
const SINK_OUTPUT: u32 = 91;

#[derive(Default)]
pub(super) struct Host {
    stamp: Option<crate::program_wire::ProgramPointSinkStampV1>,
}

impl ProgramPointSinkHostV1 for Host {
    /// Имитирует успешную установку, сохраняя точный pending stamp.
    fn try_install(
        &mut self,
        intent: ProgramPointSinkIntentV1,
    ) -> Result<(), ProgramPointSinkHostErrorV1> {
        if self
            .stamp
            .is_some_and(|stamp| intent.expected_stamp() != stamp)
        {
            return Err(ProgramPointSinkHostErrorV1::Rejected);
        }
        self.stamp = Some(intent.desired_stamp());
        Ok(())
    }
}

/// Строит минимальный Program V1 с отдельно заданным ожидаемым композитом.
fn point_wire(source: Srgb8, opacity: f64, expected: Srgb8) -> Vec<u8> {
    let mut builder = ProgramWireBuilderV1::new();
    builder
        .source(1, source)
        .fixed_target(2, 1)
        .surface_input_port(6)
        .opacity_input(5, opacity)
        .solid_paint(3, 2)
        .opacity_paint(4, 3, 5)
        .input_surface(7, 6)
        .source_over_occurrence(OCCURRENCE, 4, 7, 64.0, 0.2, 2)
        .presentation_root(ROOT, OCCURRENCE)
        .presentation_target(ROOT, OCCURRENCE)
        .exact_visible_unary(true, 10, OCCURRENCE, expected)
        .output(OUTPUT, 4);
    builder.finish().unwrap()
}

/// Ожидаемые конечные байты задаёт тест, а не проверяемый классификатор.
pub(super) fn point_attachment_for(
    source: Srgb8,
    opacity: f64,
    expected: Srgb8,
) -> ProgramAttachmentV1<Host> {
    compile_program_wire_v1(&point_wire(source, opacity, expected))
        .unwrap()
        .attach(7, OUTPUT, SINK_OUTPUT, ROOT, OCCURRENCE, Host::default())
        .unwrap()
}
