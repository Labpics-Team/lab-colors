//! Локальный потребитель одного точечного выхода: ничего не рисует и не выдаёт Observed.
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
    fn try_install(&mut self, intent: ProgramPointSinkIntentV1<'_>) -> Result<(), Error> {
        let expected = intent.expected_stamp();
        let desired = intent.desired_stamp();
        if intent.sink_output() != Some(self.output)
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
        calls: Rc<Cell<usize>>,
    }
    impl ProgramPointSinkHostV1 for Controlled {
        fn try_install(&mut self, intent: ProgramPointSinkIntentV1<'_>) -> Result<(), Error> {
            self.calls.set(self.calls.get() + 1);
            if self.reject.get() {
                return Err(Error::Rejected);
            }
            self.sink.borrow_mut().try_install(intent)
        }
    }
    fn wire() -> Vec<u8> {
        let value: Value = serde_json::from_str(include_str!("../examples/declared-point.json"))
            .expect("контрольный declared-point.json должен быть корректным JSON");
        value["programWireHex"]
            .as_str()
            .expect("контрольный programWireHex должен быть строкой")
            .as_bytes()
            .chunks_exact(2)
            .map(|s| {
                u8::from_str_radix(
                    std::str::from_utf8(s).expect("пара байтов контрольного hex должна быть UTF-8"),
                    16,
                )
                .expect("контрольная hex-пара должна задавать байт")
            })
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
        let calls = Rc::new(Cell::new(0));
        let host = || Controlled {
            sink: sink.clone(),
            reject: reject.clone(),
            calls: calls.clone(),
        };
        assert!(
            compile_program_wire_v1(&wire())
                .expect("контрольный wire должен компилироваться до отказа неверной привязке")
                .attach(7, 1000, 91, 9, 8, host())
                .is_err()
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(snapshot(&sink.borrow()), (None, None, None));
        let mut attachment = compile_program_wire_v1(&wire())
            .expect("контрольный wire должен компилироваться для проверки восстановления")
            .attach(7, 17, 91, 9, 8, host())
            .expect("точная привязка выхода должна создать attachment");
        let observed = [ProgramScenarioV1::new(1, vec![Srgb8::new([128, 128, 127])])];
        attachment
            .update_observed(1, &observed)
            .expect("первое допустимое наблюдение должно установиться");
        let old = snapshot(&sink.borrow());
        let render = attachment.current_render();
        let count = calls.get();
        assert!(
            attachment
                .update_observed(2, &[ProgramScenarioV1::new(1, vec![])])
                .is_err()
        );
        assert_eq!(calls.get(), count);
        assert_eq!(snapshot(&sink.borrow()), old);
        reject.set(true);
        assert!(attachment.update_unknown(2, 1).is_err());
        assert_eq!(attachment.current_render(), render);
        assert_eq!(snapshot(&sink.borrow()), old);
        reject.set(false);
        attachment
            .update_unknown(2, 1)
            .expect("тот же отзыв после снятия отказа host должен установиться");
        assert!(sink.borrow().point.is_none());
        assert!(attachment.current_render().is_none());
        attachment
            .update_observed(3, &observed)
            .expect("допустимое наблюдение после отзыва должно восстановить выход");
        assert!(sink.borrow().point.is_some());
        assert_eq!(sink.borrow().revision, Some(3));
        assert_ne!(sink.borrow().stamp, old.0);
    }
    #[test]
    fn stale_stamp_foreign_epoch_and_wrong_output_are_refused_without_writes() {
        struct ProbeHost {
            sink: Rc<RefCell<ModeledSink>>,
            foreign_epoch: Option<u64>,
            stale_checks: Rc<Cell<usize>>,
            foreign_checks: Rc<Cell<usize>>,
            wrong_output_checks: Rc<Cell<usize>>,
        }

        impl ProgramPointSinkHostV1 for ProbeHost {
            fn try_install(&mut self, intent: ProgramPointSinkIntentV1<'_>) -> Result<(), Error> {
                let mut target = self.sink.borrow_mut();
                target.try_install(intent)?;
                let before = snapshot(&target);
                if intent.expected_stamp() != intent.desired_stamp() {
                    assert!(target.try_install(intent).is_err());
                    assert_eq!(snapshot(&target), before);
                    self.stale_checks.set(self.stale_checks.get() + 1);
                }
                let mut wrong = ModeledSink::new(1000);
                assert!(wrong.try_install(intent).is_err());
                assert_eq!(snapshot(&wrong), (None, None, None));
                self.wrong_output_checks
                    .set(self.wrong_output_checks.get() + 1);
                if let Some(epoch) = self.foreign_epoch {
                    assert_ne!(epoch, intent.expected_stamp().binding_epoch());
                    let mut foreign = ModeledSink::new(91);
                    foreign.stamp = Some(ProgramPointSinkStampV1::new(
                        intent.expected_stamp().sequence(),
                        epoch,
                    ));
                    let before = snapshot(&foreign);
                    assert!(foreign.try_install(intent).is_err());
                    assert_eq!(snapshot(&foreign), before);
                    self.foreign_checks.set(self.foreign_checks.get() + 1);
                }
                Ok(())
            }
        }

        let sink = Rc::new(RefCell::new(ModeledSink::new(91)));
        let stale_checks = Rc::new(Cell::new(0));
        let foreign_checks = Rc::new(Cell::new(0));
        let wrong_output_checks = Rc::new(Cell::new(0));
        let host = |sink, foreign_epoch| ProbeHost {
            sink,
            foreign_epoch,
            stale_checks: Rc::clone(&stale_checks),
            foreign_checks: Rc::clone(&foreign_checks),
            wrong_output_checks: Rc::clone(&wrong_output_checks),
        };
        let observed = [ProgramScenarioV1::new(1, vec![Srgb8::new([128, 128, 127])])];
        let mut a = compile_program_wire_v1(&wire())
            .expect("контрольный wire первого attachment должен компилироваться")
            .attach(7, 17, 91, 9, 8, host(Rc::clone(&sink), None))
            .expect("точная привязка должна создать первый attachment");
        a.update_observed(1, &observed)
            .expect("первое наблюдение должно опубликоваться до проверки stale stamp");
        let epoch = sink
            .borrow()
            .stamp
            .expect("опубликованный sink должен иметь stamp для проверки чужого epoch")
            .binding_epoch();
        a.update_observed(2, &observed)
            .expect("следующая ревизия того же attachment должна установиться");
        a.update_unknown(3, 1)
            .expect("допустимый отзыв первого attachment должен установиться");
        let mut b = compile_program_wire_v1(&wire())
            .expect("контрольный wire второго attachment должен компилироваться")
            .attach(
                7,
                17,
                91,
                9,
                8,
                host(Rc::new(RefCell::new(ModeledSink::new(91))), Some(epoch)),
            )
            .expect("точная привязка должна создать второй attachment с новым epoch");
        b.update_observed(1, &observed)
            .expect("наблюдение второго attachment должно пройти до проверки чужого epoch");
        assert!(stale_checks.get() >= 2);
        assert_eq!(foreign_checks.get(), 1);
        assert!(wrong_output_checks.get() >= 3);
    }
}
