use std::collections::HashSet;

const MAX_CHUNKS: usize = 8;
const CHUNK_CHARS: usize = 4_000;
const MAX_CONTEXT_CHARS: usize = 24_000;

pub struct RetrievedContext {
    pub prompt: String,
    pub chunk_count: usize,
}

pub fn build_cited_prompt(markdown: &str, question: Option<&str>) -> RetrievedContext {
    let chunks = split_chunks(markdown);
    let terms = question.map(search_terms).unwrap_or_default();
    let mut ranked = chunks
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let folded = text.to_lowercase();
            let score = terms
                .iter()
                .filter(|term| folded.contains(term.as_str()))
                .count();
            (score, index, text)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

    let mut used = 0usize;
    let selected = ranked
        .into_iter()
        .take(MAX_CHUNKS)
        .filter_map(|(_, index, text)| {
            let remaining = MAX_CONTEXT_CHARS.saturating_sub(used);
            if remaining == 0 {
                return None;
            }
            let bounded = text.chars().take(remaining).collect::<String>();
            used += bounded.chars().count();
            Some(format!("[문서 근거 {}]\n{}", index + 1, bounded))
        })
        .collect::<Vec<_>>();

    let instruction = question
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            format!(
                "아래 문서 근거만 사용해 질문에 답하세요. 각 핵심 주장 뒤에 [문서 근거 N]을 표시하고, 근거가 없으면 모른다고 답하세요.\n질문: {}",
                value.trim()
            )
        })
        .unwrap_or_else(|| {
            "아래 문서의 핵심 내용, 주요 수치, 결론을 한국어로 간결하게 요약하세요. 각 항목 뒤에 [문서 근거 N]을 표시하세요."
                .to_string()
        });

    RetrievedContext {
        prompt: format!("{instruction}\n\n{}", selected.join("\n\n")),
        chunk_count: selected.len(),
    }
}

fn split_chunks(markdown: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for paragraph in markdown.split("\n\n") {
        if current.chars().count() + paragraph.chars().count() > CHUNK_CHARS
            && !current.trim().is_empty()
        {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        if paragraph.chars().count() > CHUNK_CHARS {
            if !current.trim().is_empty() {
                chunks.push(current.trim().to_string());
                current.clear();
            }
            let chars = paragraph.chars().collect::<Vec<_>>();
            for part in chars.chunks(CHUNK_CHARS) {
                chunks.push(part.iter().collect::<String>());
            }
        } else {
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(paragraph);
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

fn search_terms(question: &str) -> HashSet<String> {
    question
        .split(|character: char| !character.is_alphanumeric())
        .map(str::trim)
        .filter(|term| term.chars().count() >= 2)
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::build_cited_prompt;

    #[test]
    fn prioritizes_question_terms_and_adds_citations() {
        let source = "첫 문단입니다.\n\n매출은 42% 증가했습니다.\n\n마지막 문단입니다.";
        let result = build_cited_prompt(source, Some("매출 증가율은?"));
        assert!(result.prompt.contains("매출은 42%"));
        assert!(result.prompt.contains("[문서 근거 1]"));
        assert!(result.chunk_count > 0);
    }
}
