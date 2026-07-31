const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"];
const MIN_MEANINGFUL_TEXT_CHARS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrDecision {
    Disabled,
    UseEmbeddedText,
    TextOcr,
    MathOcr,
    Unsupported,
}

pub fn decide(
    extension: &str,
    extracted_text: &str,
    ocr_enabled: bool,
    math_ocr_enabled: bool,
    has_math_hint: bool,
) -> OcrDecision {
    if !ocr_enabled {
        return OcrDecision::Disabled;
    }
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    if IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        return OcrDecision::TextOcr;
    }
    if extension != "pdf" {
        return OcrDecision::Unsupported;
    }
    if meaningful_text_chars(extracted_text) >= MIN_MEANINGFUL_TEXT_CHARS {
        return OcrDecision::UseEmbeddedText;
    }
    if math_ocr_enabled && has_math_hint {
        OcrDecision::MathOcr
    } else {
        OcrDecision::TextOcr
    }
}

fn meaningful_text_chars(text: &str) -> usize {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .take(MIN_MEANINGFUL_TEXT_CHARS)
        .count()
}

#[cfg(test)]
mod tests {
    use super::{decide, OcrDecision};

    #[test]
    fn keeps_normal_pdf_text_without_ocr() {
        assert_eq!(
            decide(
                "pdf",
                "이 문서는 정상적인 텍스트 레이어를 충분히 포함하고 있습니다.",
                true,
                true,
                true
            ),
            OcrDecision::UseEmbeddedText
        );
    }

    #[test]
    fn recognizes_all_promised_image_extensions_locally() {
        for extension in ["jpg", "png", "webp", "bmp", "tiff"] {
            assert_eq!(
                decide(extension, "", true, false, false),
                OcrDecision::TextOcr
            );
        }
    }

    #[test]
    fn separates_math_ocr_and_respects_the_master_switch() {
        assert_eq!(
            decide("pdf", "", true, true, true),
            OcrDecision::MathOcr
        );
        assert_eq!(
            decide("pdf", "", false, true, true),
            OcrDecision::Disabled
        );
    }
}
