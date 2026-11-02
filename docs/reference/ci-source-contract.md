# Исходник и recipe CI

**Diátaxis:** reference.

CI исполняет разрешённые исходники Labpics. Native repository ID и PR head
проверяются до checkout в обязательном `source-admission`. Foreign head получает
`REFUSED`; отсутствующая или неверная native identity — `INCOMPLETE`.

Запись admission различает SHA события, PR head, caller и выполняемый recipe,
его repository/path/ref, run/attempt/check-run и раннер. Поля `job.workflow_*`
идентифицируют файл, определяющий job; `github.workflow_*` относятся к caller.
Запись допуска не означает успешную сборку или квалификацию всего workflow.

В LOCAL bootstrap caller и recipe сохраняют один snapshot. Существующая
проверка `ci_workflow_binding.py` сравнивает байты caller, CI worker и package
worker с этим snapshot; отдельно сохраняется точный checkout SHA. Временный
native FULLSHA admission относится только к reviewed immutable recipe.

Тяжёлый job требует self-hosted environment до исполнения исходников. Другой
раннер даёт явный отказ. OS/toolchain/consumer проверки остаются обязательными;
metadata smoke не заменяет Android, Apple, WASM или consumer result. Лёгкий
publisher использует GitHub-hosted environment для npm trusted OIDC.

`full-domain-run.yml` с `guard_only=true` выполняет только native identity smoke
и отказ injected foreign head в LOCAL reusable worker. Такой run не создаёт
full-domain proof. Значение по умолчанию сохраняет полное доказательство.

До npm publish сохраняются точные успешные main/push runs, реальные обязательные
jobs и все исполнившиеся referenced recipes, привязанные к release SHA. Успех
другого recipe или source не квалифицирует изменённый граф и toolchain pins.

Bootstrap recipe явно отказывает перед npm publish. Разрешение выпуска появляется
только в отдельно квалифицированном protected-main срезе.
