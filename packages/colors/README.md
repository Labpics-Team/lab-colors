# @labpics/colors

Компилятор и runtime проверяемых цветовых Program-графов.

Клиент передаёт канонические Program wire-байты. Ядро атомарно проверяет граф, создаёт Session, принимает наблюдения среды и возвращает сертифицированные Paint outputs либо, через отдельный attached-маршрут FV-01, stamp-bound полномочие на уже установленную материализацию. Recipe-меню ролей и специальные Material/Glow runtime-пути удалены из публичного API.

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

## Attached materialization FV-01

`compileAttachedProgramWire` компилирует те же канонические Program bytes, но не создаёт полномочие сам по себе. `CompiledAttachedProgram.attach(...)` связывает exact output/presentation scope с переданным приложением `AttachedPointSinkHostV1`. Host остаётся внешним адаптером и не поставляется пакетом как второй sink-продукт.

Каждый `updateObserved` или `updateUnknown` проходит через stamp protocol с `bindingEpoch` и последовательностью. Меняющая материализацию команда несёт `expectedSequence` и `desiredSequence`; host обязан либо установить весь новый scope атомарно, либо оставить прежние CSS/state/stamp неизменными. Отказ host возвращается typed и не commit'ит Session. `confirm-exact` только сверяет уже опубликованное состояние.

`Ready` update может выдать opaque `AttachedMaterializationAuthority` через `takeAuthority(index)`. Authority создаётся только из post-commit attachment, связывает Program identity, published revision, sink sequence/binding epoch и terminal presentation. `ProgramSnapshot`, Paint, отдельный sRGB8 и CSS-строка не являются входом конструктора authority.

`runtime.validateAuthority(authority)` типизированно отвергает старую revision, другой Program identity, другую owner generation и другой host binding epoch. `Stale` и `Failed` не выдают materialization authority. Типы FV-01 provisional до 1.0 и могут сужаться вместе с контрактом.

Важно: `RendererProvenanceV1` в этом срезе имеет только `Unverified`. Authority удостоверяет **attached modeled materialization**, а не реально наблюдённые пиксели браузера и не научное измерение renderer. Browser proof отдельно сверяет записанные source sRGB8 + opacity через `getComputedStyle` как host-binding evidence и независимо пересчитывает modeled source-over composite. Filters, occlusion, backdrop effects, rasterization, color-management конкретного renderer и renderer identity этим срезом не сертифицируются. Их нельзя выводить из `Unverified` authority.

## Контракт

- Один authoring-root: канонический Program wire. Detached evidence и attached materialization являются двумя явными проекциями одного Program, а не двумя движками.
- `compileProgramWire` -> `ProgramRuntime` -> `ProgramSnapshot` возвращает detached evidence-only snapshot.
- `compileAttachedProgramWire` -> `CompiledAttachedProgram` -> `AttachedProgramRuntime` выдаёт authority только после успешного атомарного host commit.
- Обновление атомарно: отказ не публикует частичный state и не отравляет следующую корректную revision.
- Output появляется только вместе с сертификатом полного hard-support. Detached output сам по себе не является полномочием на материализацию и не гарантирует конечный видимый результат приложения.
- Канонические байты имеют одну `ContentIdentity`; appearance context входит в identity соответствующего Program.
- Невалидные wire-байты, графы, observation, bindings, stamps и resource bounds возвращают typed-отказы; fallback отсутствует.
- DOM и CSS не входят в Core. FV-01 host adapter принадлежит приложению или proof harness, а не package runtime.

## Числовые входы

ID потока и причины, число подложек и индекс output принимаются только как
конечные целые `number` в диапазоне `0..4294967295`. Ревизия принимается как
`bigint` в диапазоне `0n..18446744073709551615n`. Строки, нечисловые значения,
дроби и выход за диапазон не приводятся к допустимому значению. Допуск входа
предшествует изменению Session; после отказа корректную ревизию можно повторить.
Допустимость конкретного индекса, формы observations и порядка ревизий
по-прежнему проверяет соответствующий контракт runtime.

## Распознавание ошибок

`isProgramError(value)` распознаёт detached и attached Program ошибки по принадлежности текущему
`Error` и допустимой паре `operation`/`code`. Неизвестное значение, чужая пара
или исключение при чтении возвращают `false`, не заменяя исходную ошибку
новым исключением. Каждое поле читается не более одного раза, без преобразования
его значения; читаемые унаследованные поля и getters допустимы.

Проверка описывает только наблюдённые значения. Она не удостоверяет происхождение
ошибки, не замораживает объект и не гарантирует неизменность следующих чтений
пользовательских getters. Непознанная ошибка остаётся у вызывающего приложения.

## Дополнительные функции

- `evaluateWcag22` - точная оценка WCAG 2.2 в finite sRGB8/Q55-профиле.
- `numericalCapabilityManifest` - манифест численных возможностей и доказательств сборки.

Публикация registry/deploy не является частью этого изменения: пакет проверяется из точного CI tarball до отдельного release-гейта.
