# Данные и атрибуция

Исходный код `labcolors-core` распространяется по лицензии MIT из файла
`LICENSE`. Встроенная таблица
`contracts/clean-set-srgb8-v1/point-clean-set-srgb8-column-rle-v1.bin`
является адаптированным набором данных и распространяется одновременно по
условиям CC-BY-4.0 и CC-BY-SA-4.0. Полные тексты находятся в
`LICENSES/CC-BY-4.0.txt` и `LICENSES/CC-BY-SA-4.0.txt`.

## Sato и Inoue

- Keiko Sato, Takaaki Inoue. *Perception of color emotions for single colors
  in red-green defective observers*. PeerJ 4:e2751 (2016).
- Источник: Data S1, DOI `10.7717/peerj.2751`.
- Лицензия источника: CC-BY-4.0.

Две сессии усреднены внутри участника, после чего участники получили равный вес
внутри объявленной когорты. Полученная постфактум граница является
версионированной политикой пакета, а не универсальным законом человеческой
«чистоты» цвета.

## CIE 1931 2° и D65

- International Commission on Illumination. *Colour-matching functions of CIE
  1931 standard colorimetric observer* (2019), DOI
  `10.25039/CIE.DS.xvudnb9b`, CC-BY-SA-4.0.
- International Commission on Illumination. *CIE standard illuminant D65*
  (2019), DOI `10.25039/CIE.DS.hjfjmt59`, CC-BY-SA-4.0.

Из официальных таблиц взяты значения стандартного наблюдателя на
`360..780 nm` и D65; объявленная геометрия суммы точек, точный вывод и конечная
sRGB8-классификация описаны в закреплённом исследовательском выпуске.

## Изменения и точное происхождение

Labpics связал источники с номинальным мостом sRGB8, выполнил точный рациональный
вывод и преобразовал итоговую таблицу в канонический column-RLE-кодек.

- research commit: `ac6d9654fc722334d8bc2054afb903770f2aad80`;
- release SHA-256:
  `67cadaae38bbaea3096dba69142b5bf3d7776b7574ec224022abbcd119c45ce6`;
- raw table SHA-256:
  `97bcc9f793adb7f13bd70c89e9788c8ab61baf8c77e9f8cd80335ad767d71ae2`;
- runtime codec SHA-256:
  `aa6aa7c0b630437f1c1ba8c2ceafb0dadf6551c42331559504076a6cd44e6331`.

Использование источников не означает одобрения Labpics их авторами или
издателями. Дополнительные гарантии сверх условий соответствующих лицензий не
предоставляются.
