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

## Дополнительные функции

- `evaluateWcag22` — точная оценка WCAG 2.2 в finite sRGB8/Q55-профиле.
- `numericalCapabilityManifest` — манифест численных возможностей и доказательств сборки.

Публикация registry/deploy не является частью этого изменения: пакет проверяется из точного CI tarball до отдельного release-гейта.
