# Решение: константы APCA в LPC — лицензионная позиция

Статус: принято · Дата: 2026-06-10 · Скоуп: `crates/labcolors-core/src/lpc.rs`
Это инженерная оценка риска, не юридическая консультация; финальное слово — за юристом.
Все цитаты проверены дословно по первоисточникам изолированной верификацией (verification-rarr, PASS, 2026-06-10).

## Контекст

LPC подаёт скорректированную по Гельмгольцу-Кольраушу яркость (Y_hk через CIECAM16) в форму контрастной кривой, опубликованную проектом APCA (экспоненты 0.56/0.57 для нормальной полярности, 0.65/0.62 для обратной, масштаб 1.14, мягкий кламп чёрного 0.022/1.414). Код из репозиториев Myndex не копировался — это независимая Rust-реализация опубликованной математики. Из-за другого входа LPC намеренно даёт другие значения, чем APCA, и является другой метрикой.

## Факты (первоисточники, получены и верифицированы 2026-06-10)

1. apca-w3 распространяется под кастомной «Limited W3 License» (поле license в npm) — гибрид W3C Software and Document License с дополнительными ограничениями и AGPL-v3-фолбэком: «Any files, or use cases of files, not under the W3 cooperative agreement are licensed under the AGPU v3 License» (sic). Условия лицензии (запрет модификации «essential elements of the code or specific approved constants, except as required to port to a given language», право аудита Myndex для коммерческих/paywalled приложений, запрещённые сферы) связывают пользователей файлов репозитория. Мы эти файлы не используем.
   https://github.com/Myndex/apca-w3/blob/master/LICENSE.md
2. Copyright США не распространяется на «any idea, procedure, process, system, method of operation, concept, principle, or discovery» (17 U.S.C. § 102(b)) — формула и константы не являются охраняемым объектом авторского права; охраняется только конкретное кодовое выражение. https://www.law.cornell.edu/uscode/text/17/102
3. Действующее ограничение — товарный знак: «APCA», «SAPC», «SACAM» заявлены как trademarks Myndex Research / Andrew Somers и могут обозначать только комплаентные, актуальные реализации. Безымянное использование Myndex разрешает в категории «Generic Perceptual Contrast: covers use of the algorithm in nonstandard applications and/or without brand identification». Примечание точности: соседнее разрешение «Unidentified use of the APCA algorithm or math is permitted» обусловлено немодифицированным кодом apca-w3 (изменения только для портирования) — LPC модифицирован, поэтому наша опора именно категория «Generic Perceptual Contrast», а не «Unidentified use».
   https://git.apcacontrast.com/documentation/minimum_compliance
4. Прецеденты: Chromium DevTools (BSD-3, `front_end/core/common/ColorUtils.ts`) и colorjs.io (MIT, `src/contrast/APCA.js`) поставляют те же константы; на crates.io существуют MIT-реализации (apca-w3, apcach-rs, egui_colors). Свидетельств enforcement-действий не найдено (отсутствие доказательств, не доказательство отсутствия).
5. Позиция автора (GitHub discussions #12, Aug 2023): ограничительная лицензия — временная мера на период публичной беты («This is a temporary situation during the public beta, and will not be permanent, and there will be a free-to-use library with a permissive license»); цель — предотвращение ложной рекламы третьими лицами (формулировка участника обсуждения romainmenke, эксплицитно подтверждённая Somers: «Yes, you are correct»).

## Решение

- Константы остаются. Пере-вывод не делаем: copyright их не охраняет, а гипотетический патентный риск пере-вывод не лечит.
- Метрика называется LPC. Строка «APCA» не должна появляться в публичных API-символах, имени метрики и маркетинговых материалах. Этот файл — единственное место атрибуции: контрастная кривая LPC использует экспоненты и кламп-константы из опубликованной формулы APCA 0.0.98G-4g; LPC не является APCA, не APCA-совместима и не комплаентна, не одобрена Myndex Research или Andrew Somers.
- Внутренняя `fn apca()` в lpc.rs переименовывается (например, `contrast_core`) — юридически безразлично, но снимает любой спор.
- LPC не предлагается и не рекламируется для medical, clinical, human-safety, aerospace, transportation, military применений.
- Опционально, не блокер: вежливое уведомление Myndex (GitHub discussions / info@readtech.org) о существовании производной метрики под другим именем.

## Остаточные риски

- «Patent(s) pending» заявлен в лицензиях Myndex; ни выданного патента, ни опубликованной заявки не найдено (формальный USPTO-поиск не выполнялся). Оценка риска: низкий; пересмотреть при любом enforcement-сигнале.
- Документы Myndex заявляют контроль над «the algorithm or math»; § 102(b) этого не поддерживает для независимой реализации, но утверждение в суде не тестировалось.
- Для справки: generic-вариант самого Somers (deltaphistar) — модифицированный AGPL v3 (AGPL + положения §7), не чистый AGPL-3.0.
