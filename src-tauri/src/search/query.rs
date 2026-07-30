use super::SearchError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    pub terms: Vec<String>,
    pub phrases: Vec<String>,
    pub excluded_terms: Vec<String>,
    pub extensions: Vec<String>,
    pub path_terms: Vec<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub near: Option<u32>,
    pub match_any: bool,
}

impl ParsedQuery {
    pub fn parse(input: &str) -> Result<Self, SearchError> {
        let mut parsed = Self::default();
        let tokens = tokenize(input)?;
        for (index, token) in tokens.iter().enumerate() {
            if token.text.is_empty() {
                continue;
            }

            if token.quoted {
                parsed.phrases.push(token.text.clone());
            } else if token.text == "OR" {
                let valid_neighbors = index > 0
                    && index + 1 < tokens.len()
                    && !is_any_operator(&tokens[index - 1])
                    && !is_any_operator(&tokens[index + 1]);
                if !valid_neighbors {
                    return Err(SearchError::invalid_query(
                        "OR must appear between positive search terms",
                    ));
                }
                parsed.match_any = true;
            } else if let Some(value) = token.text.strip_prefix("ext:") {
                for extension in value.split(',') {
                    parsed.extensions.push(validate_extension(extension)?);
                }
            } else if let Some(value) = token.text.strip_prefix("path:") {
                if value.is_empty() {
                    return Err(SearchError::invalid_query("path filter cannot be empty"));
                }
                parsed.path_terms.push(value.to_owned());
            } else if let Some(value) = token.text.strip_prefix("after:") {
                validate_date(value)?;
                parsed.after = Some(value.to_owned());
            } else if let Some(value) = token.text.strip_prefix("before:") {
                validate_date(value)?;
                parsed.before = Some(value.to_owned());
            } else if let Some(value) = token.text.strip_prefix('~') {
                let distance = value
                    .parse::<u32>()
                    .map_err(|_| SearchError::invalid_query("near distance must be a number"))?;
                if !(1..=100).contains(&distance) {
                    return Err(SearchError::invalid_query(
                        "near distance must be between 1 and 100",
                    ));
                }
                if parsed.near.replace(distance).is_some() {
                    return Err(SearchError::invalid_query(
                        "only one near operator is allowed",
                    ));
                }
            } else if let Some(value) = token.text.strip_prefix('-') {
                if value.is_empty() {
                    parsed.terms.push(token.text.clone());
                } else {
                    parsed.excluded_terms.push(value.to_owned());
                }
            } else {
                parsed.terms.push(token.text.clone());
            }
        }
        if let (Some(after), Some(before)) = (parsed.after.as_deref(), parsed.before.as_deref()) {
            if after > before {
                return Err(SearchError::invalid_query(
                    "after date cannot follow before date",
                ));
            }
        }
        if parsed.near.is_some() && parsed.terms.len() + parsed.phrases.len() < 2 {
            return Err(SearchError::invalid_query(
                "near search requires at least two positive terms or phrases",
            ));
        }
        if parsed.match_any && parsed.terms.len() + parsed.phrases.len() < 2 {
            return Err(SearchError::invalid_query(
                "OR requires at least two positive search terms",
            ));
        }
        Ok(parsed)
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
            && self.phrases.is_empty()
            && self.excluded_terms.is_empty()
            && self.extensions.is_empty()
            && self.path_terms.is_empty()
            && self.after.is_none()
            && self.before.is_none()
    }

    pub(crate) fn fts_match_expression(&self, include_filename: bool) -> Option<String> {
        let positive = self
            .phrases
            .iter()
            .chain(self.terms.iter())
            .map(|value| fts_literal(value))
            .collect::<Vec<_>>();

        if positive.is_empty() {
            return None;
        }

        let positive_expression = if let Some(distance) = self.near.filter(|_| positive.len() > 1) {
            format!("NEAR({}, {distance})", positive.join(" "))
        } else if self.match_any {
            format!("({})", positive.join(" OR "))
        } else {
            positive.join(" AND ")
        };
        let positive_expression = if include_filename {
            positive_expression
        } else {
            format!("{{title body}} : ({positive_expression})")
        };

        let exclusions = self
            .excluded_terms
            .iter()
            .map(|value| {
                let literal = fts_literal(value);
                if include_filename {
                    literal
                } else {
                    format!("{{title body}} : {literal}")
                }
            })
            .collect::<Vec<_>>();
        if exclusions.is_empty() {
            Some(positive_expression)
        } else {
            Some(format!(
                "{positive_expression} NOT {}",
                exclusions.join(" NOT ")
            ))
        }
    }

    pub(crate) fn excluded_fts_expressions(&self, include_filename: bool) -> Vec<String> {
        self.excluded_terms
            .iter()
            .map(|value| {
                let literal = fts_literal(value);
                if include_filename {
                    literal
                } else {
                    format!("{{title body}} : {literal}")
                }
            })
            .collect()
    }
}

fn is_any_operator(token: &Token) -> bool {
    !token.quoted && token.text == "OR"
}

pub(crate) fn validate_date(value: &str) -> Result<(), SearchError> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err(SearchError::invalid_query(
            "dates must use the YYYY-MM-DD format",
        ));
    }
    let year = parse_date_part(&value[0..4])?;
    let month = parse_date_part(&value[5..7])?;
    let day = parse_date_part(&value[8..10])?;
    if year == 0 || !(1..=12).contains(&month) {
        return Err(SearchError::invalid_query(
            "date is outside the valid range",
        ));
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > max_day {
        return Err(SearchError::invalid_query(
            "date is outside the valid range",
        ));
    }
    Ok(())
}

pub(crate) fn validate_extension(value: &str) -> Result<String, SearchError> {
    let normalized = value.trim().trim_start_matches('.').to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > 16
        || !normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(SearchError::invalid_query(
            "extensions must contain only ASCII letters and numbers",
        ));
    }
    Ok(normalized)
}

fn parse_date_part(value: &str) -> Result<u32, SearchError> {
    value
        .parse()
        .map_err(|_| SearchError::invalid_query("date contains non-numeric characters"))
}

fn fts_literal(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[derive(Debug)]
struct Token {
    text: String,
    quoted: bool,
}

fn tokenize(input: &str) -> Result<Vec<Token>, SearchError> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut in_quotes = false;
    let mut whole_token_quoted = false;
    let mut quoted_path = false;
    let mut quoted_characters = 0_usize;
    let mut characters = input.chars().peekable();

    while let Some(character) = characters.next() {
        if character == '\\' && in_quotes {
            match characters.peek().copied() {
                Some('"') => {
                    characters.next();
                    text.push('"');
                }
                Some('\\') if !(quoted_path && quoted_characters == 0) => {
                    characters.next();
                    text.push('\\');
                }
                _ => text.push('\\'),
            }
            quoted_characters += 1;
            continue;
        }
        if character == '"' {
            if !in_quotes {
                whole_token_quoted = text.is_empty();
                quoted_path = text == "path:";
                quoted_characters = 0;
            }
            in_quotes = !in_quotes;
            continue;
        }
        if character.is_whitespace() && !in_quotes {
            if !text.is_empty() {
                tokens.push(Token {
                    text: std::mem::take(&mut text),
                    quoted: whole_token_quoted,
                });
                whole_token_quoted = false;
            }
        } else {
            text.push(character);
            if in_quotes {
                quoted_characters += 1;
            }
        }
    }
    if in_quotes {
        return Err(SearchError::invalid_query("quoted phrase is not closed"));
    }
    if !text.is_empty() {
        tokens.push(Token {
            text,
            quoted: whole_token_quoted,
        });
    }
    Ok(tokens)
}
