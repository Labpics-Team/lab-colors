# Обновить измеренный предел WASM

Предел в `packages/colors/bench/wasm.json` относится к raw WASM канонической
Linux x64 сборки. Его можно увеличить для принятого расширения продукта после
измерения точных байтов; произвольный запас запрещён. Gzip остаётся
диагностикой доставки.

## Получить измерение

Откройте PR с изменением и дождитесь job `wasm build + headless test + size`.
После двух одинаковых сборок этот job сохраняет артефакт
`colors-wasm-measurement-<sourceSha>-attempt-<attempt>` на три дня, до проверки
предела размера. Превышение предела по-прежнему завершает job ошибкой.
Отдельная сборка или обход size gate не нужны.

Скачайте артефакт выбранного запуска:

```sh
gh run view "$RUN_ID" --repo Labpics-Team/lab-colors \
  --json headSha,attempt,url,conclusion
gh run download "$RUN_ID" --repo Labpics-Team/lab-colors \
  --name "colors-wasm-measurement-$SOURCE_SHA-attempt-$ATTEMPT" \
  --dir "$MEASUREMENT_DIR"
```

Внутри находятся `labcolors_bg.wasm`, `wasm-build.json` и `measurement.json`. Сверьте SHA-256 и
длину файла с `artifact`, а `sourceSha`, номер запуска и попытку — с job и его
checkout. У pull request checkout может быть merge-коммитом, а не head ветки.
Receipt также содержит хеши workflow, Cargo.lock, manifests, скриптов сборки и
старого budget. `wasm-build.json` создаётся общим build-скриптом после успешной
компиляции и оптимизации. Measurement требует совпадения записей обеих сборок,
хешей входов и конечных байтов, проверяет фактический профиль из сообщений Cargo
и версии инструментов. Базовый профиль WASM задан в `recipe.cargoProfile`.
Скрипт перекрывает одноимённые profile settings, incremental и rustflags
средствами Cargo. Сообщения Cargo проверяются у WASM-крейта и его target-зависимостей.
Обычная локальная сборка также создаёт
`wasm-build.json`; право на каноническое измерение остаётся у Linux x64 CI.

Receipt проверяет пять полей профиля, доступных в Cargo JSON. Дополнительные
Cargo config, package overrides для остальных полей, linkers и wrappers
остаются доверенными входами. Повторная сборка подтверждает воспроизводимость
внутри данного CI job. Границы проверки и локального source SHA описаны
в [инструкции сборки](develop.md).

`qualification: measurement-only` означает только воспроизводимое измерение.
Оно не удостоверяет прохождение package, browser или publish проверок. Публикация
по-прежнему принимает свой отдельный проверенный npm-архив.

## Принять новый предел

Обоснуйте изменение размера добавленной возможностью продукта. В том же PR:

1. Запишите `measurement.source` как `github-actions-run-<RUN_ID>`, а
   `measurement.rawBytes` и `policy.maxRawBytes` — как точное
   `artifact.bytes` из receipt. Оставьте канонический JSON с отступом в два пробела
   и завершающим переводом строки.
2. Если рецепт или инструменты изменились, перенесите именно измеренные значения;
   не смешивайте байты одной сборки с описанием другой.
   Изменение рецепта само по себе сохраняет прежний измеренный предел. Новый
   размер и источник записываются только после нового канонического измерения.
3. Вычислите `sha256sum packages/colors/bench/wasm.json` и обновите только
   `WASM_BUDGET_FILE_SHA256` в `scripts/check-wasm-size-budget.mjs`.
4. Приложите к PR источник измерения, run attempt, source SHA и WASM SHA-256.
   Сохраните скачанные байты для review до окончания короткого срока хранения.
5. Запустите проверки контракта и дождитесь обычного CI нового коммита:

```sh
node --test packages/colors/test/wasm-size-budget.test.mjs \
  packages/colors/test/wasm-measurement.test.mjs
python3 scripts/test_ci_storage.py
```

Новый коммит собирается заново тем же рецептом и обязан уложиться в измеренный
предел. Привязка producer к source SHA может изменить байты WASM; равенство нового
файла прежнему измерению заранее не предполагается. Gate нельзя делать условным,
заменять gzip-порогом или разрешать его ошибку через `continue-on-error`.
