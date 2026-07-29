use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VisibleFilter {
    pub field: String,
    pub operator: String,
    pub value: String,
}

pub fn explain_text_query(query: &str) -> Vec<VisibleFilter> {
    query
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| VisibleFilter {
            field: "text".to_string(),
            operator: "contains".to_string(),
            value: token.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_terms_are_visible_as_filters() {
        let filters = explain_text_query("dark metallic impact");

        assert_eq!(filters.len(), 3);
        assert_eq!(filters[0].value, "dark");
    }
}
