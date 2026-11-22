# Опубликовать пример цветовой системы

Публичный пример поставляется статическим OCI-образом. Образ принадлежит Colors,
а Kubernetes-маршрут, размещение и доступ к нему — Labpics Infra.

## Собрать выпуск

После успешного npm-выпуска workflow [color demo](../../.github/workflows/color-demo.yml)
скачивает точную версию `@labpics/colors` через `npm pack`. Он сверяет metadata
пакета с commit тега `colors-v<version>` и проверяет длину и SHA-256 WASM.
Сборка Core и workspace-подстановка в этот путь не входят.

Workflow проверяет фактический HTTP-ответ контейнера с read-only filesystem:
точные байты HTML, JS, CSS, JSON и WASM, MIME и отказ для отсутствующего ресурса.
После этого он публикует amd64/arm64-образ в
`ghcr.io/labpics-team/lab-colors-demo`.
Повторный выпуск примера с уже опубликованным пакетом запускается через
`workflow_dispatch` на `main`.

Артефакт `color-demo-image` содержит `demo-image.json`: неизменяемый image digest,
commit примера, источник npm-выпуска, npm integrity и размер/SHA WASM.
`release.json` с идентичностью пакета входит и в сам сайт.

## Доставить через Infra

Следуйте действующей [инструкции публикации Labpics Infra](https://github.com/Labpics-Team/labpics-infra/blob/main/docs/how-to/deploy-service.md).
В PR инфраструктуры обновите image **по digest** из `demo-image.json`.
Объявление сервиса задаёт собственные namespace, Service, HTTPRoute, ресурсы,
readiness и сетевую границу; ApplicationSet обнаруживает его каталог `k8s/`.
Владельцем применения остаётся Argo CD.

После reconcile сопоставьте применённую Git-ревизию и image digest, готовность
Deployment и условия HTTPRoute. На публичном адресе проверьте TLS, `release.json`,
байты WASM, загрузку ресурсов и реальные изменения цвета в браузере. HTTP-проверка
образа не заменяет внешний маршрут, доступ из сетей аудитории и поведение SDK.
Откат — возврат предыдущего digest через тот же PR-путь.
