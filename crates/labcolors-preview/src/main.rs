use labcolors_core::neutral::{CurveParams, NeutralCurve};
use labcolors_core::scale::AccentCurve;
use labcolors_core::ViewingConditions;
use std::fs;

const BAR_W: u32 = 400;
const BAR_H: u32 = 40;
const MARGIN: u32 = 40;
const COL_GAP: u32 = 48;
const SAMPLES: u32 = 400;

fn sample_steps(curve: &impl labcolors_core::ColorCurve) -> Vec<String> {
    curve.sample_hex(SAMPLES as usize)
}

fn main() {
    let light = NeutralCurve::new("#FFFFFF", "#787880", "#101012").unwrap();
    let dim = NeutralCurve::with_vc(
        "#FFFFFF",
        "#787880",
        "#101012",
        &CurveParams::default(),
        &ViewingConditions::dim_surround(),
    )
    .unwrap();

    let curves: Vec<(&str, Vec<String>, Vec<String>)> = vec![
        ("Neutral", sample_steps(&light), sample_steps(&dim)),
        (
            "Blue (#007AFF)",
            sample_steps(&AccentCurve::new("#007AFF", &light).unwrap()),
            sample_steps(&AccentCurve::new("#007AFF", &dim).unwrap()),
        ),
        (
            "Green (#34C759)",
            sample_steps(&AccentCurve::new("#34C759", &light).unwrap()),
            sample_steps(&AccentCurve::new("#34C759", &dim).unwrap()),
        ),
        (
            "Orange (#FF9500)",
            sample_steps(&AccentCurve::new("#FF9500", &light).unwrap()),
            sample_steps(&AccentCurve::new("#FF9500", &dim).unwrap()),
        ),
    ];

    let total_w = MARGIN * 2 + BAR_W * 2 + COL_GAP;
    let row_h = BAR_H + 26;
    let total_h = MARGIN + 4 + curves.len() as u32 * row_h;

    let mut s = String::new();
    s.push_str(&format!(
        r##"<svg xmlns='http://www.w3.org/2000/svg' width='{}' height='{}'>
<style>text{{font-family:system-ui,-apple-system,sans-serif}}</style>
<text x='{}' y='{}' font-size='11' fill='{}'>Light theme</text>
<text x='{}' y='{}' font-size='11' fill='{}'>Dark theme</text>
"##,
        total_w, total_h,
        MARGIN, MARGIN, "#999",
        MARGIN + BAR_W + COL_GAP, MARGIN, "#999",
    ));

    for (i, (name, light_c, dim_c)) in curves.iter().enumerate() {
        let y = MARGIN + 12 + i as u32 * row_h;

        s.push_str(&format!(
            "<text x='{}' y='{}' font-size='11' fill='{}'>{}</text>\n",
            MARGIN, y - 3, "#666", name
        ));

        // Light bar
        let bx = MARGIN;
        s.push_str(&format!(
            "<rect x='{}' y='{}' width='{}' height='{}' fill='none' stroke='{}' stroke-width='0.5'/>\n",
            bx, y, BAR_W, BAR_H, "#ddd"
        ));
        for (ci, col) in light_c.iter().enumerate() {
            s.push_str(&format!(
                "<rect x='{}' y='{}' width='1' height='{}' fill='{}'/>\n",
                bx + ci as u32, y, BAR_H, col
            ));
        }

        // Dark bar
        let bx2 = MARGIN + BAR_W + COL_GAP;
        s.push_str(&format!(
            "<rect x='{}' y='{}' width='{}' height='{}' fill='none' stroke='{}' stroke-width='0.5'/>\n",
            bx2, y, BAR_W, BAR_H, "#ddd"
        ));
        for (ci, col) in dim_c.iter().enumerate() {
            s.push_str(&format!(
                "<rect x='{}' y='{}' width='1' height='{}' fill='{}'/>\n",
                bx2 + ci as u32, y, BAR_H, col
            ));
        }
    }

    s.push_str("</svg>\n");

    fs::write("docs/palette.svg", &s).expect("write palette.svg");
    eprintln!("Written docs/palette.svg ({} bytes)", s.len());
}
