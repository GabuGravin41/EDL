use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub file: String,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Span {
    pub fn new(
        file: impl Into<String>,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Self {
        Self {
            file: file.into(),
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    pub fn merge(&self, other: &Span) -> Span {
        Span {
            file: self.file.clone(),
            start_line: self.start_line,
            start_col: self.start_col,
            end_line: other.end_line,
            end_col: other.end_col,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub category: String,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub entities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl Diagnostic {
    pub fn error(
        code: impl Into<String>,
        category: impl Into<String>,
        message: impl Into<String>,
        span: Option<Span>,
    ) -> Self {
        Self {
            code: code.into(),
            category: category.into(),
            severity: Severity::Error,
            message: message.into(),
            entities: Vec::new(),
            span,
            measured: None,
            required: None,
            unit: None,
            suggestion: None,
        }
    }

    pub fn with_entities(mut self, entities: Vec<String>) -> Self {
        self.entities = entities;
        self
    }

    pub fn with_measurement(
        mut self,
        measured: f64,
        required: f64,
        unit: impl Into<String>,
    ) -> Self {
        self.measured = Some(measured);
        self.required = Some(required);
        self.unit = Some(unit.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}
