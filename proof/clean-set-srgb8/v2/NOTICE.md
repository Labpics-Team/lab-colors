# Данные, код и атрибуция

Код распространяется по MIT; полный текст в `LICENSES/MIT.txt`.
Научные данные и их адаптации сохраняют применимые CC-BY-4.0 и
CC-BY-SA-4.0 условия; полные тексты включены в `LICENSES/`.

- Keiko Sato, Takaaki Inoue. *Perception of color emotions for single colors
  in red-green defective observers*. PeerJ 4:e2751 (2016), DOI
  `10.7717/peerj.2751`; Data S1 `10.7717/peerj.2751/supp-3`, Table S1
  `10.7717/peerj.2751/supp-2`. CC-BY-4.0, подтверждено license статьи XML.
- International Commission on Illumination. *Colour-matching functions
  of CIE 1931 standard colorimetric observer* (2019), DOI
  `10.25039/CIE.DS.xvudnb9b`. CC-BY-SA-4.0, согласно метаданным источника.
- International Commission on Illumination. *CIE standard illuminant D65*
  (2019), DOI `10.25039/CIE.DS.hjfjmt59`. CC-BY-SA-4.0, согласно метаданным.

Первичные ZIP, DOCX, XML и CSV сохранены без изменения. Labpics получил
точные cohort means и условную связь stimulus ID, выделил 360..780 нм,
связал источники с закреплённым номинальным binary64 sRGB8-мостом,
вывел постфактум объявленную границу и преобразовал конечную таблицу
в канонический column-RLE кодек. Производная таблица распространяется
с соблюдением CC-BY-4.0 и CC-BY-SA-4.0; код MIT не отменяет эти условия.

Расхождение light yellow между XYZ в Data S1 и xyY в Table S1 сохранено
в `NOMINAL-SPEC.md`. Модель принимает буквальные параметры Table S1;
причина расхождения и физическая идентичность стимула не установлены.

Источники и авторы не заявляли одобрения этой политики Labpics. Новая
derivation не подтверждает утраченное историческое происхождение v1.
Применимость и сохранённые исходные неоднозначности описаны в
`NOMINAL-SPEC.md`; дополнительных гарантий к лицензиям не предоставляется.
