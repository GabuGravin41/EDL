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
pub struct RepairAction {
    /// Machine-readable repair verb: e.g. "adjust_offset", "move_fixture", "reverse_swing",
    /// "add_window", "add_opening", "connect_pin", "fix_voltage", "add_constraint", "split_net".
    pub action: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_value: Option<String>,
    pub rationale: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair: Option<RepairAction>,
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
            repair: None,
        }
    }

    pub fn warning(
        code: impl Into<String>,
        category: impl Into<String>,
        message: impl Into<String>,
        span: Option<Span>,
    ) -> Self {
        Self {
            code: code.into(),
            category: category.into(),
            severity: Severity::Warning,
            message: message.into(),
            entities: Vec::new(),
            span,
            measured: None,
            required: None,
            unit: None,
            suggestion: None,
            repair: None,
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

    pub fn with_repair(mut self, repair: RepairAction) -> Self {
        self.repair = Some(repair);
        self
    }

    /// Deterministically synthesize a machine-readable `RepairAction` from this diagnostic
    /// if one was not explicitly attached yet.
    pub fn inferred_repair(&self) -> RepairAction {
        if let Some(ref r) = self.repair {
            return r.clone();
        }
        let target = self
            .entities
            .first()
            .cloned()
            .unwrap_or_else(|| "project".to_string());
        let rationale = self
            .suggestion
            .clone()
            .unwrap_or_else(|| self.message.clone());

        match self.code.as_str() {
            "E_VERIFY_JAMB_MARGIN" => RepairAction {
                action: "adjust_offset".to_string(),
                target,
                field: Some("offset".to_string()),
                suggested_value: self.required.map(|r| format!(">= {:.0}mm", r)),
                rationale,
            },
            "E_VERIFY_DOOR_SWING_COLLISION" | "E_VERIFY_DOOR_SWING_WALL" | "E_VERIFY_DOOR_SWING_NOTCH" => {
                RepairAction {
                    action: "reverse_swing".to_string(),
                    target,
                    field: Some("hinge_or_swing".to_string()),
                    suggested_value: Some("flip hinge or swing target room".to_string()),
                    rationale,
                }
            }
            "E_VERIFY_DOOR_SWING_FIXTURE" | "E_VERIFY_FIXTURE_BOUNDS"
            | "E_VERIFY_FIXTURE_CLEARANCE" | "E_VERIFY_FIXTURE_OVERLAP"
            | "E_VERIFY_FIXTURE_BLOCKS_OPENING" | "E_VERIFY_FIXTURE_IN_NOTCH" => RepairAction {
                action: "move_fixture".to_string(),
                target: self.entities.get(1).cloned().unwrap_or(target),
                field: Some("offset_or_at".to_string()),
                suggested_value: None,
                rationale,
            },
            "E_VERIFY_UNREACHABLE_ROOM" => RepairAction {
                action: "add_opening".to_string(),
                target,
                field: Some("host".to_string()),
                suggested_value: Some("shared_wall(...) door or passage".to_string()),
                rationale,
            },
            "E_VERIFY_NATURAL_LIGHT" => RepairAction {
                action: "add_window".to_string(),
                target,
                field: Some("host".to_string()),
                suggested_value: Some("exterior(<room>, <side>)".to_string()),
                rationale,
            },
            "E_ERC_UNCONNECTED_PIN" => RepairAction {
                action: "connect_pin".to_string(),
                target,
                field: Some("connect".to_string()),
                suggested_value: Some("add pin to appropriate net".to_string()),
                rationale,
            },
            "E_ERC_VOLTAGE_MISMATCH" => RepairAction {
                action: "fix_voltage".to_string(),
                target,
                field: Some("net_or_regulator".to_string()),
                suggested_value: self.required.map(|r| format!("{:.2}V", r)),
                rationale,
            },
            "E_ERC_POWER_GROUND_SHORT" | "E_ERC_MULTIPLE_POWER_DRIVERS" => RepairAction {
                action: "split_net".to_string(),
                target,
                field: Some("connect".to_string()),
                suggested_value: Some("separate conflicting power/ground pins".to_string()),
                rationale,
            },
            "E_ERC_SINGLE_PIN_NET" => RepairAction {
                action: "connect_or_remove_net".to_string(),
                target,
                field: Some("connect".to_string()),
                suggested_value: Some("connect >= 2 pins or mark pin nc".to_string()),
                rationale,
            },
            "E_SOLVER_UNDERCONSTRAINED" => RepairAction {
                action: "add_constraint".to_string(),
                target,
                field: Some("layout".to_string()),
                suggested_value: Some("add require constraint for remaining DOF".to_string()),
                rationale,
            },
            _ => RepairAction {
                action: "edit_source".to_string(),
                target,
                field: None,
                suggested_value: None,
                rationale,
            },
        }
    }
}
