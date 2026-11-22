# Совместное обновление контекста и цветов

Когда приложение владеет поверхностью и цветами её содержимого, оно может
применить новый контекст целиком через `ProgramRuntime.updateObservedWith`:

```ts
let candidate: ProgramSnapshot | undefined;
try {
  const accepted = runtime.updateObservedWith(
    revision, scenarioIds, surfaceRgb, surfaceCount,
    snapshot => {
      candidate = snapshot;
      if (snapshot.state !== "ready") return false;
      const colors = snapshot.copyOutputTable();
      return host.applyAtomically({ surfaceRgb, colors });
    },
  );
  if (accepted) revision += 1n;
} finally {
  candidate?.free();
}
```

Поиск, окончательная проверка и создание независимого снимка завершаются до
вызова callback. `true` принимает подготовленную ревизию без повторного поиска
и возвращает тот же объект `ProgramSnapshot`. `false` возвращает `undefined`;
исключение callback пробрасывается без подмены. В обоих случаях отказа прежняя
ревизия Session остаётся принятой и номер предложения можно использовать снова.
Callback должен возвращать именно синхронный boolean. Иное значение, включая
Promise, даёт `program_host_protocol`.

Приложение копирует нужные данные и завершает все операции, способные отказать,
до подтверждения внешней записи. Атомарность `host.applyAtomically` принадлежит
приложению: при `false` или исключении фон и цвета должны остаться прежними.
Сам runtime не отменяет произвольные записи callback в DOM. Для уже случившегося
изменения внешней среды предназначен `updateObserved`; отказ вычисления не
возвращает такую среду к прошлому состоянию.

Callback получает owned-снимок. Если он сохранён, вызывающий код освобождает его
после завершения `updateObservedWith`, в том числе при отказе. Снимок сохраняет
свои значения после следующих обновлений runtime. Внутри callback запрещены
повторное обновление того же runtime, его освобождение и освобождение переданного
снимка; публичный пакет возвращает `program_busy`. Данные снимка доступны для
чтения во время callback. Запрета на обновление другого независимого runtime нет.

Rust-потребитель использует `ProgramSessionV1::prepare_observed_view` и
`PreparedProgramUpdateV1::commit_with`. Drop подготовленного обновления отменяет
предложение. `commit` применяет уже вычисленное обновление, когда внешнего
согласования не требуется.
