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
    pub(crate) fn new(output: u32) -> Self {
        Self {
            output,
            stamp: None,
            point: None,
            revision: None,
        }
    }
}
impl ProgramPointSinkHostV1 for ModeledSink {
    fn try_install(&mut self, intent: ProgramPointSinkIntentV1) -> Result<(), Error> {
        let expected = intent.expected_stamp();
        let desired = intent.desired_stamp();
        if intent.sink_output() != self.output
            || expected.binding_epoch() != desired.binding_epoch()
            || desired.binding_epoch() == 0
            || self.stamp.is_some_and(|s| s != expected)
            || (self.stamp.is_none() && expected.sequence() != 0)
            || self.revision.is_some_and(|r| intent.revision() < r)
        {
            return Err(Error::Rejected);
        }
        match intent.operation() {
            ProgramPointSinkOperationV1::SetAll => {
                if intent.point().is_none()
                    || expected.sequence().checked_add(1) != Some(desired.sequence())
                {
                    return Err(Error::Protocol);
                }
            }
            ProgramPointSinkOperationV1::RevokeAll => {
                if intent.point().is_some()
                    || expected.sequence().checked_add(1) != Some(desired.sequence())
                {
                    return Err(Error::Protocol);
                }
            }
            ProgramPointSinkOperationV1::ConfirmExact => {
                if self.point != intent.point() || desired != expected {
                    return Err(Error::Rejected);
                }
            }
            _ => return Err(Error::Protocol),
        }
        // Полный пакет проверен. Единственная запись происходит после всех отказов.
        self.stamp = Some(desired);
        self.point = intent.point();
        self.revision = Some(intent.revision());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use labcolors_core::{
        Srgb8,
        program_wire::{ProgramScenarioV1, compile_program_wire_v1},
    };
    use serde_json::Value;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    struct Controlled {
        sink: Rc<RefCell<ModeledSink>>,
        reject: Rc<Cell<bool>>,
        calls: Rc<RefCell<Vec<ProgramPointSinkIntentV1>>>,
    }
    impl ProgramPointSinkHostV1 for Controlled {
        fn try_install(&mut self, intent: ProgramPointSinkIntentV1) -> Result<(), Error> {
            self.calls.borrow_mut().push(intent);
            if self.reject.get() {
                return Err(Error::Rejected);
            }
            self.sink.borrow_mut().try_install(intent)
        }
    }
    fn wire() -> Vec<u8> {
        let value: Value =
            serde_json::from_str(include_str!("../examples/declared-point.json")).unwrap();
        value["programWireHex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|s| u8::from_str_radix(std::str::from_utf8(s).unwrap(), 16).unwrap())
            .collect()
    }
    fn snapshot(
        sink: &ModeledSink,
    ) -> (
        Option<ProgramPointSinkStampV1>,
        Option<ProgramPaintOutputV1>,
        Option<u64>,
    ) {
        (sink.stamp, sink.point, sink.revision)
    }
    #[test]
    fn refused_binding_update_and_install_preserve_the_local_sink_and_recover() {
        let sink = Rc::new(RefCell::new(ModeledSink::new(91)));
        let reject = Rc::new(Cell::new(false));
        let calls = Rc::new(RefCell::new(Vec::new()));
        let host = || Controlled {
            sink: sink.clone(),
            reject: reject.clone(),
            calls: calls.clone(),
        };
        assert!(
            compile_program_wire_v1(&wire())
                .unwrap()
                .attach(7, 1000, 91, 9, 8, host())
                .is_err()
        );
        assert!(calls.borrow().is_empty());
        assert_eq!(snapshot(&sink.borrow()), (None, None, None));
        let mut attachment = compile_program_wire_v1(&wire())
            .unwrap()
            .attach(7, 17, 91, 9, 8, host())
            .unwrap();
        let observed = [ProgramScenarioV1::new(1, vec![Srgb8::new([128, 128, 127])])];
        attachment.update_observed(1, &observed).unwrap();
        let old = snapshot(&sink.borrow());
        let render = attachment.current_render();
        let count = calls.borrow().len();
        assert!(
            attachment
                .update_observed(2, &[ProgramScenarioV1::new(1, vec![])])
                .is_err()
        );
        assert_eq!(calls.borrow().len(), count);
        assert_eq!(snapshot(&sink.borrow()), old);
        reject.set(true);
        assert!(attachment.update_unknown(2, 1).is_err());
        assert_eq!(attachment.current_render(), render);
        assert_eq!(snapshot(&sink.borrow()), old);
        reject.set(false);
        attachment.update_unknown(2, 1).unwrap();
        assert!(sink.borrow().point.is_none());
        assert!(attachment.current_render().is_none());
        attachment.update_observed(3, &observed).unwrap();
        assert!(sink.borrow().point.is_some());
        assert_eq!(sink.borrow().revision, Some(3));
        assert_ne!(sink.borrow().stamp, old.0);
    }
    #[test]
    fn stale_stamp_foreign_epoch_and_wrong_output_are_refused_without_writes() {
        let capture = Rc::new(RefCell::new(Vec::new()));
        let make = |calls: Rc<RefCell<Vec<ProgramPointSinkIntentV1>>>| {
            let sink = Rc::new(RefCell::new(ModeledSink::new(91)));
            let host = Controlled {
                sink,
                reject: Rc::new(Cell::new(false)),
                calls,
            };
            let mut a = compile_program_wire_v1(&wire())
                .unwrap()
                .attach(7, 17, 91, 9, 8, host)
                .unwrap();
            a.update_observed(
                1,
                &[ProgramScenarioV1::new(1, vec![Srgb8::new([128, 128, 127])])],
            )
            .unwrap();
            a
        };
        let mut a = make(capture.clone());
        let first = capture.borrow()[0];
        a.update_observed(
            2,
            &[ProgramScenarioV1::new(1, vec![Srgb8::new([128, 128, 127])])],
        )
        .unwrap();
        a.update_unknown(3, 1).unwrap();
        let mut target = ModeledSink::new(91);
        for intent in capture.borrow().iter() {
            target.try_install(*intent).unwrap();
        }
        let before = snapshot(&target);
        assert!(target.try_install(first).is_err());
        assert_eq!(snapshot(&target), before);
        let foreign = Rc::new(RefCell::new(Vec::new()));
        let _b = make(foreign.clone());
        assert!(target.try_install(foreign.borrow()[0]).is_err());
        assert_eq!(snapshot(&target), before);
        let mut wrong = ModeledSink::new(1000);
        assert!(wrong.try_install(first).is_err());
        assert_eq!(snapshot(&wrong), (None, None, None));
    }
}
