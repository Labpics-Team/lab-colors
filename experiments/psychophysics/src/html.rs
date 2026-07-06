//! Рендер автономного HTML-раннера: шаблон + встроенный манифест.
//!
//! Шаблон `runner/template.html` компилируется в бинарник (`include_str!`), token
//! `__SESSION_JSON__` заменяется компактным JSON манифеста. Результат — единый
//! самодостаточный файл: без сети, CDN и `fetch` (который на `file://` блокирует
//! CORS), поэтому наблюдателю достаточно открыть его в браузере.

use crate::stimulus::Manifest;

/// Токен-заполнитель в шаблоне.
const TOKEN: &str = "__SESSION_JSON__";

/// Шаблон раннера, вкомпилированный в бинарник.
const TEMPLATE: &str = include_str!("../runner/template.html");

/// Отрендерить автономный HTML для данного манифеста.
#[must_use]
pub fn render(manifest: &Manifest) -> String {
    TEMPLATE.replace(TOKEN, &manifest.to_json_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color;
    use crate::passport::Family;
    use crate::stimulus::{Acceptance, DesignParams, build_session};

    fn demo() -> Manifest {
        let f: Vec<Family> = ["#FF3B30", "#007AFF"]
            .iter()
            .map(|&hex| Family {
                key: hex.to_string(),
                anchor_hex: hex.to_string(),
                anchor_rgb: color::hex_to_rgb(hex).unwrap(),
            })
            .collect();
        build_session(
            &f,
            DesignParams::default(),
            Acceptance::default(),
            "#101012",
            3,
        )
    }

    #[test]
    fn template_contains_token() {
        assert!(
            TEMPLATE.contains(TOKEN),
            "шаблон потерял заполнитель {TOKEN}"
        );
    }

    #[test]
    fn render_injects_and_removes_token() {
        let html = render(&demo());
        assert!(!html.contains(TOKEN), "заполнитель не заменён");
        assert!(html.contains("PAIR_CROSSOVER_Y"));
        assert!(html.contains("const SESSION ="));
    }

    #[test]
    fn injected_json_is_valid() {
        // Вырезаем встроенный JSON и парсим его — гарантия, что раннер получит
        // валидный литерал.
        let html = render(&demo());
        let start = html.find("const SESSION = ").unwrap() + "const SESSION = ".len();
        let tail = &html[start..];
        let end = tail.find(";\n").unwrap();
        let json = &tail[..end];
        let parsed = crate::json::parse(json).expect("встроенный JSON валиден");
        assert_eq!(
            parsed.get("target").unwrap().as_str().unwrap(),
            "PAIR_CROSSOVER_Y"
        );
    }

    #[test]
    fn no_external_resources() {
        // Автономность: ни http-ссылок на ресурсы, ни fetch/CDN.
        let html = render(&demo());
        assert!(!html.contains("http://"), "внешний http-ресурс");
        assert!(!html.contains("https://"), "внешний https-ресурс");
        assert!(!html.contains("src=\"http"), "внешний скрипт/картинка");
    }
}
