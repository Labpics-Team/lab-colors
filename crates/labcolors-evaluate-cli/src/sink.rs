//! Локальный потребитель полного intent: ничего не рисует и не выдаёт Observed.
use labcolors_core::program_wire::{
    ProgramPaintOutputV1, ProgramPointSinkHostErrorV1 as Error, ProgramPointSinkHostV1,
    ProgramPointSinkIntentV1, ProgramPointSinkOperationV1, ProgramPointSinkStampV1,
};

pub(crate) struct ModeledSink {
    output: u32,
    stamp: Option<ProgramPointSinkStampV1>,
    point: Option<ProgramPaintOutputV1>,
    revision: Option<u64>,
}
impl ModeledSink {
    pub(crate) fn new(output:u32)->Self { Self { output, stamp:None, point:None, revision:None } }
}
impl ProgramPointSinkHostV1 for ModeledSink {
    fn try_install(&mut self,intent:ProgramPointSinkIntentV1)->Result<(),Error> {
        let expected=intent.expected_stamp(); let desired=intent.desired_stamp();
        if intent.sink_output()!=self.output || expected.binding_epoch()!=desired.binding_epoch()
            || desired.binding_epoch()==0 || self.stamp.is_some_and(|s|s!=expected)
            || (self.stamp.is_none() && expected.sequence()!=0)
            || self.revision.is_some_and(|r|intent.revision()<r) {
            return Err(Error::Rejected);
        }
        match intent.operation() {
            ProgramPointSinkOperationV1::SetAll => {
                if intent.point().is_none() { return Err(Error::Protocol); }
            },
            ProgramPointSinkOperationV1::RevokeAll => {
                if intent.point().is_some() { return Err(Error::Protocol); }
            },
            ProgramPointSinkOperationV1::ConfirmExact => {
                if self.point!=intent.point() { return Err(Error::Rejected); }
            },
            _ => return Err(Error::Protocol),
        }
        // Полный пакет проверен. Единственная запись происходит после всех отказов.
        self.stamp=Some(desired); self.point=intent.point(); self.revision=Some(intent.revision());
        Ok(())
    }
}
