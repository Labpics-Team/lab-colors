//! LPC channel separation per roadmap §F1 exit criterion #4.
//! Each evaluator report carries zero or more channel-tagged observations.
//! Channels are mutually exclusive evidence containers — no implicit summation.
//!
//! WIRE NOTE: No serde derives. See labcolors-wasm for wire mirror types.

/// Channel-separated evidence within an evaluator report.
/// Each variant is a self-contained observation; consumers MUST NOT
/// sum across channels without an admitted joint decision model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LpcChannelReportV1 {
    /// Photometric luminance contrast evidence.
    Photometric {
        luminance_contrast: f64,
        note: Option<&'static str>,
    },
    /// Chromatic hue/chroma difference evidence.
    Chromatic {
        hue_difference_degrees: f64,
        chroma_difference: f64,
    },
    /// Typography measurement evidence.
    Typography {
        font_size_px: f64,
        weight_class: u16,
        x_height_ratio: Option<f64>,
    },
    /// Context field scalar value.
    ContextField { field_id: &'static str, value: f64 },
    /// Appearance mode and adaptation state.
    Appearance {
        mode_id: &'static str,
        adaptation_state: &'static str,
    },
    /// WCAG 2.1 contrast ratio evidence. Produced ONLY by externally-admitted
    /// WCAG21 evaluators. Core WCAG22 evaluators do NOT produce this channel.
    Wcag21 {
        contrast_ratio: f64,
        threshold_level: &'static str,
        passes: bool,
    },
    /// APCA (Accessible Perceptual Contrast Algorithm) evidence. Produced ONLY
    /// by externally-admitted APCA evaluators. Never produced by core evaluators.
    Apca {
        lc_value: f64,
        polarity: &'static str,
        font_size_px: Option<f64>,
        weight_class: Option<u16>,
    },
    /// Legacy readability model evidence. Admitted ONLY through external profile
    /// gate with explicit deprecation notice. Consumers SHOULD treat as informational.
    LegacyReadability {
        model_id: &'static str,
        score: f64,
        deprecated_notice: &'static str,
    },
}
