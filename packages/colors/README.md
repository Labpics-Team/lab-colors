# @labpics/colors

Компилятор и runtime проверяемых цветовых Program-графов.

Клиент передаёт канонические Program wire-байты. Ядро атомарно проверяет граф, создаёт Session, принимает наблюдения среды и возвращает только сертифицированные Paint outputs. Recipe-меню ролей и специальные Material/Glow runtime-пути удалены из публичного API.

## Установка

```sh
npm install @labpics/colors
```

## Первый маршрут

```ts
import init, { compileProgramWire } from "@labpics/colors";
import {
  ProgramWireBuilderV1,
  SURROUND_AVERAGE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1,
} from "@labpics/colors/program-wire/abi-v1.js";

await init();
const programBytes = new ProgramWireBuilderV1()
  .source(11, [20, 20, 20])
  .fixedTarget(21, 11)
  .surfaceInputPort(31)
  .solidPaint(41, 21)
  .inputSurface(51, 31)
  .sourceOverOccurrence(61, 41, 51, 64, 0.2, SURROUND_AVERAGE_V1)
  .presentationRoot(71, 61)
  .presentationTarget(71, 61)
  .wcag22VisibleUnary(true, 81, 61, WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1)
  .output(91, 41)
  .finish();
const runtime = compileProgramWire(programBytes, 1);
try {
  const snapshot = runtime.updateObserved(
    1n,
    new Uint32Array([1]),
    new Uint8Array([255, 255, 255]),
    1,
  );
  try {
    if (snapshot.state === "ready") {
      for (let index = 0; index < snapshot.outputCount(); index += 1) {
        console.log(
          snapshot.outputSlot(index),
          snapshot.outputRgb(index),
          snapshot.outputOpacity(index),
        );
      }
    }
  } finally {
    snapshot.free();
  }
} finally {
  runtime.free();
}
```

Канонические `LCPW` v1 bytes создаются `ProgramWireBuilderV1` или другим реализационно-независимым энкодером того же контракта.

## Подключение к внешнему sink

Для сценария, где приложение само владеет DOM или другим renderer-surface,
используйте `attachProgramWire`. Синхронный `host` получает полный typed intent
(`setAll`, `revokeAll` или `confirmExact`) и должен вернуть `true` только после
атомарной установки всего принадлежащего ему scope; `false` отклоняет update.
Пакет не пишет DOM и не выдаёт authority из исходного или промежуточного Paint.

```ts
const attachment = attachProgramWire(programBytes, 1, 91, 501, 71, 61, (intent) => {
  // Приложение проверяет sinkOutput, expectedSequence и bindingEpoch,
  // затем одним действием обновляет собственный renderer-surface.
  return applyOwnedPointIntent(intent);
});
try {
  const snapshot = attachment.updateObserved(
    1n,
    new Uint32Array([1]),
    new Uint8Array([255, 255, 255]),
    1,
  );
  try {
    if (snapshot.state === "ready") {
      const authority = attachment.materializationAuthority();
      try {
        console.log(authority.terminalCompositeRgb());
      } finally {
        authority.free();
      }
    }
  } finally {
    snapshot.free();
  }
} finally {
  // Сначала отзовите scope у внешнего владельца, затем подтвердите dispose.
  try {
    attachment.dispose(true);
  } finally {
    attachment.free();
  }
}
```

`materializationAuthority()` читает только текущий успешно установленный
attachment head. Его `rendererProvenance` остаётся `"unverified"`: результат —
модельная terminal materialization с проверенными identity, revision, sink stamp,
binding epoch и отсутствием downstream point, а не доказательство browser paint
или человеческого восприятия. При отказе host предыдущий head сохраняется.
Пока host callback исполняется синхронно, повторный вызов любой операции attachment,
включая `free()`, получает typed-отказ busy; освобождайте attachment после возврата
из callback. `free()` также получает typed-отказ `program_attachment_revoke_unconfirmed`,
пока внешний scope не отозван и `dispose(true)` не завершился успешно. Внешний scope
отзывается владельцем host до `dispose(true)`.

## Инспекция certificate envelope

`decodeCertificateEnvelope(bytes)` разбирает только фиксированный `LCEN` v1
transport envelope и возвращает `UntrustedCertificateEnvelopeV1`. Результат —
метаданные framing и проверенные digest, а не authority result и не доказательство
истины payload. Тело payload остаётся opaque и не передаётся в JavaScript.

До вызова WASM-функции фасад проверяет `bytes.byteLength` и отвергает размер больше
`MAX_CERTIFICATE_ENVELOPE_BYTES` (`2097152`), поскольку `wasm-bindgen` копирует
typed array в linear memory до входа Rust. Неизвестная schema, authority,
operation, payload type/version, неканоничные строки, trailing bytes и digest
mismatch дают typed error; `isCertificateError(value)` проверяет только
допустимую пару `operation`/`code`.

Создать envelope из произвольных JS-байтов, `ProgramSnapshot`, Paint, CSS, DOM или
materialization authority нельзя. Producer capability и admission ledger остаются
владением Rust Core; в r13 нет persistence, network relay, CLI, science authority
или registry publication.

## Контракт

- Один публичный runtime-root: `compileProgramWire` → `ProgramRuntime` → `ProgramSnapshot`.
- Обновление атомарно: отказ не публикует частичный state.
- Output появляется только вместе с сертификатом полного hard-support. Это внутренняя проверка в рамках Session, а не публичный переносимый сертификат или гарантия конечного видимого результата приложения.
- Канонические байты имеют одну `ContentIdentity`.
- Невалидные wire-байты, графы, observation и resource bounds возвращают typed-отказы; fallback отсутствует.
- DOM и CSS не входят в ядро. Применение output принадлежит приложению.

## Числовые входы

ID потока и причины, число подложек и индекс output принимаются только как
конечные целые `number` в диапазоне `0..4294967295`. Ревизия принимается как
`bigint` в диапазоне `0n..18446744073709551615n`. Строки, нечисловые значения,
дроби и выход за диапазон не приводятся к допустимому значению. Допуск входа
предшествует изменению Session; после отказа корректную ревизию можно повторить.
Допустимость конкретного индекса, формы observations и порядка ревизий
по-прежнему проверяет соответствующий контракт runtime.

## Распознавание ошибок

`isProgramError(value)` распознаёт ошибки Program по принадлежности текущему
`Error` и допустимой паре `operation`/`code`. Неизвестное значение, чужая пара
или исключение при чтении возвращают `false`, не заменяя исходную ошибку
новым исключением. Каждое поле читается не более одного раза, без преобразования
его значения; читаемые унаследованные поля и getters допустимы.

Проверка описывает только наблюдённые значения. Она не удостоверяет происхождение
ошибки, не замораживает объект и не гарантирует неизменность следующих чтений
пользовательских getters. Непознанная ошибка остаётся у вызывающего приложения.

## Дополнительные функции

- `evaluateWcag22` — точная оценка WCAG 2.2 в finite sRGB8/Q55-профиле.
- `numericalCapabilityManifest` — манифест численных возможностей и доказательств сборки.

Публикация registry/deploy не является частью этого изменения: пакет проверяется из точного CI tarball до отдельного release-гейта.
