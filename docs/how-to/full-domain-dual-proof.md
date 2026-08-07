# How-to: полнодоменное дуальное доказательство

Маршрут: один полнодоменный прогон обоих движков → 512 полос верификации →
одна печать дуального доказательства.

В `main` исполнимы шаги 1–4. Печати нет: `.github/workflows/dual-proof.yml` и
`proof/region/v1/tests/dual_proof_gate.py` приходят с PR 549 (шаг 5).

Нужен `gh`, авторизованный в репозитории: все шаги — ручной `workflow_dispatch`.
Код проверяйте только в WSL или Linux (см. «Грабли»).

## 1. Прогон обоих движков на полном домене

```sh
gh workflow run full-domain-run.yml
gh run list --workflow full-domain-run.yml --limit 1 \
  --json databaseId --jq '.[0].databaseId'
```

Один прогон — две задачи, конверт каждой 480 мин. Замер прогона `31116022208`:
Arb 63 мин, MPFI 43 мин.

Прогон выгружает по артефакту на движок: `verification-evidence-arb` и
`verification-evidence-mpfi`. Идентификатор прогона (дальше `EVIDENCE_RUN_ID`) —
вход всех следующих шагов.

Перед диспатчем убедитесь, что артефакты живы:

```sh
gh api repos/{owner}/{repo}/actions/runs/EVIDENCE_RUN_ID/artifacts \
  --jq '.artifacts[] | [.name, (.expired|tostring)] | @tsv'
# verification-evidence-arb   false
# verification-evidence-mpfi  false
```

`true` — полоса такой артефакт уже не скачает, нужен новый прогон.

## 2. План полос

Сеть не нужна, план лежит в коде:

```sh
python3 proof/region/v1/corpus_dispatch.py --mode plan --out plan-out
# plan lanes=256 lane_width=65536 shard_width=16384 domain_points=16777216
```

Пишет `plan-out/dispatch-plan.json`.

## 3. Диспатч полос — отдельно для каждого движка

Сначала печать:

```sh
python3 proof/region/v1/corpus_dispatch.py --mode verification-dispatch \
  --evidence-run-id EVIDENCE_RUN_ID \
  --evidence-artifact verification-evidence-arb \
  --dry-run --out plan-out
```

Ровно 256 строк, окна от 0 до 16711680 шагом 65536. Пересчитайте строки. Живой
запуск — та же команда без `--dry-run`. Повторите обе с
`verification-evidence-mpfi`: всего 512 прогонов.

`--out` обязателен во всех режимах, хотя в режимах диспатча ничего не пишет.
`--evidence-artifact` принимает только два имени выше; на чужом имени `main`
падает с `TypeError: 'ShardCorpusRejectedV1' object is not iterable`, кампания
не стартует.

Полоса (`verification-lanes.yml`, конверт 480 мин) скачивает названный артефакт
из `EVIDENCE_RUN_ID`, требует в нём `job.bin` и
`comparator-bundle/comparator-manifest-v2.bin`, проигрывает своё окно через
`corpus_lane.py` под компаратором движка и выгружает
`verification-lane-<window_start>-<window_points>`.

## 4. Сбор идентификаторов 512 прогонов

`gh workflow run` не возвращает id, а в `main` у полос нет `run-name`, поэтому
единственный различитель кампаний — дата:

```sh
gh run list --workflow verification-lanes.yml --status success \
  --created ГГГГ-ММ-ДД --limit 512 \
  --json databaseId --jq '[.[].databaseId] | join(",")'
```

Прогоны прошлых кампаний в этом списке ничем не отличаются от текущих. Сверьте
число: 512.

## 5. Печать дуального доказательства — только с PR 549

В `main` печати нет. По диффу 549:

- workflow `dual-proof.yml`, вход `lane_run_ids` — список из шага 4 через запятую;
- одна задача, конверт 330 мин, пересобирает оба движка заново: расписки
  source-bound не имеют wire-формы, держать обе может только выпустивший их процесс;
- покрытие проверяется до сборки: ровно две `comparator_source_identity`, каждая
  покрывает `[0, 16777216)` без дыр и нахлёстов;
- печатает `proof/region/v1/tests/dual_proof_gate.py`, результат — артефакт
  `dual-proof-identity`.

Полосы должны быть выпущены версией `verification-lanes.yml` из 549: там имя
артефакта содержит движок. У полос из `main` оба движка дают одинаковые имена, и
сбор отказывает по коллизии.

## Что меняется в незавершённых PR

Все пять ветвятся от `main` независимо, поэтому порядок мержа значим: 552 уже
несёт полярность из 548, а 553 и 555 всё ещё написаны под `--dry-run`.

**PR 548 — полярность флага.** `--dry-run` исчезает. Печать становится
поведением по умолчанию, живой запуск — явный `--execute`, обязательно с
`--expect-lanes 256`; при несовпадении с планом — отказ до первого прогона.
`--execute` вместе с `--mode plan` отвергается. Типизированный отказ больше не
роняет цикл `TypeError`, а перед первым диспатчем координатор спрашивает GitHub,
несёт ли прогон названный артефакт и не истёк ли он; неудача наблюдения — тоже
отказ. После 548 команда шага 3 без `--dry-run` ничего не запустит, а с
`--dry-run` упадёт на разборе аргументов.

**PR 549 — печать и имена.** Появляются `.github/workflows/dual-proof.yml` и
`proof/region/v1/tests/dual_proof_gate.py`. У полосы появляется `run-name` с
координатами, а имя её артефакта становится
`verification-lane-<evidence_artifact>-<window_start>-<window_points>`.

**PR 552 — происхождение прогона.** Допуск проверяет прогон до артефакта:
workflow `.github/workflows/full-domain-run.yml`, событие `workflow_dispatch`,
статус `completed`, вывод `success`; `head_sha` сообщается оператору, а не
решается здесь. Имя артефакта ничего не говорит о том, откуда он: тот же
workflow из форка публикует артефакт ровно допустимого имени.

**PR 553 — сбор идентификаторов.** Режим `--mode collect` (плюс `--run-limit`,
по умолчанию 2000) читает заголовки прогонов из `run-name` PR 549 и пишет
`<--out>/lane-runs.json` схемы `corpus-lane-runs-v1`: `run_id` на окно. Имя
файла одно, поэтому для двух движков нужны разные `--out` — иначе второй
перезапишет первый. Отказ несёт списки `missing` и `duplicated` окон. Шаг 4
перестаёт быть догадкой по времени; зависит от 549.

**PR 555 — содержимое артефакта.** Живой путь скачивает названный артефакт
целиком и требует внутри `job.bin` и
`comparator-bundle/comparator-manifest-v2.bin`. Прогон, сделанный до нынешнего
`evidence-out/`, проходит проверку имени и умирает во всех 256 полосах на первом
`test -f`.

## Грабли

**Диспатч без флага печати.** Забытый `--dry-run` рассылает весь план боевыми
прогонами: так одна ошибка превратилась в 133 мусорных запуска. Порядок —
печать, счёт строк (256), живой запуск. PR 548 переворачивает полярность именно
поэтому.

**Проверять на Windows нельзя.** `proof/region/v1/executor.py:13` импортирует
`fcntl`, которого на Windows нет, поэтому слой `arb`/`mpfi` не грузится, а
discovery молча отдаёт меньший набор. Замер на нетронутом дереве `e420e21`:
Windows — 258 тестов и 7 ошибок импорта, WSL — 408 тестов и 3 предсуществующих
падения в `test_build`. Проверяйте в WSL:

```sh
python3 -m unittest discover -s proof/region/v1/tests -p "test_*.py"
export PATH="$HOME/.cargo/bin:$PATH" && cargo +stable test -p labcolors-core
```

**Инвентарь тестов пинится в двух местах.**
`proof/region/v1/arb/tests/gate.py` держит `EXPECTED_TEST_INVENTORY_SHA256`, а
`proof/region/v1/tests/test_build.py` — собственные `ARB_INVENTORY_SHA256_V1`,
`ARB_ORDER_SHA256_V1` и `ARB_TEST_COUNT_V1`. Второй набор — независимый внешний
оракул: он ничего не импортирует из гейта, поэтому правка одного гейта проходит
его собственную проверку и падает потом в задаче `test` (`ci-worker.yml`). Так
уже было: CI-прогон `31125436887` ветки PR 549, задача `CI / test`,
`AssertionError: 270 != 267` в `test_build.py:291`.

Обновляйте оба набора значениями из исполнения гейта в WSL:

```sh
python3 - <<'PY'
import hashlib, sys
sys.path.insert(0, "proof/region/v1/arb/tests")
import gate

ids = tuple(t.id() for t in gate.iter_tests_v1(gate.full_suite_v1()))
print("ARB_TEST_COUNT_V1      ", len(ids))
print("ARB_INVENTORY_SHA256_V1", gate.test_inventory_sha256_v1(gate.full_suite_v1()))
print("ARB_ORDER_SHA256_V1    ",
      hashlib.sha256(b"".join(i.encode("utf-8") + b"\n" for i in ids)).hexdigest())
PY
```

`ARB_INVENTORY_SHA256_V1` обязан совпасть с `EXPECTED_TEST_INVENTORY_SHA256`.

**Правка `crates/labcolors-core/src/lib.rs` ломает две аттестации.** Любая,
вплоть до комментария: обе считают точные байты файла. Перегенерируйте четыре
артефакта по цепочке, каждый следующий из предыдущего:

1. запись `crates/labcolors-core/src/lib.rs` (поля `bytes` и `sha256`) в
   `crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.json`;
2. `crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.sha256`;
3. `EXPECTED_SOURCE_CAPSULE_SHA256` в `scripts/verify_point_support_surplus.py`;
4. `crates/labcolors-core/contracts/point-support-reference-surplus-q55-bps-proof-v1.json`
   — только через `python3 scripts/verify_point_support_surplus.py --emit`.

Проверьте обоими верификаторами:

```sh
python3 scripts/verify_clean_set_receipt.py product --product-root "$PWD"
python3 scripts/verify_point_support_surplus.py
```

Сверьте результат с `origin/main` поле за полем: меняться должны только digest-ы
над исходником. Ни одно число сертификата меняться не имеет права.
