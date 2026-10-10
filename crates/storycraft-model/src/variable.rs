use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Id;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Variable {
    pub id: Id,
    /// Name used in `%Name%` text references. Unique within a project.
    pub name: String,
    /// Initial value; also fixes the variable's type.
    pub value: Value,
}

impl Variable {
    pub fn new(name: impl Into<String>, value: Value) -> Self {
        Variable { id: Id::new(), name: name.into(), value }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Text(String),
    Number(f64),
    Bool(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Text,
    Number,
    Bool,
}

impl ValueKind {
    pub const ALL: [ValueKind; 3] = [ValueKind::Text, ValueKind::Number, ValueKind::Bool];

    pub fn label(&self) -> &'static str {
        match self {
            ValueKind::Text => "Text",
            ValueKind::Number => "Number",
            ValueKind::Bool => "True/False",
        }
    }

    pub fn default_value(&self) -> Value {
        match self {
            ValueKind::Text => Value::Text(String::new()),
            ValueKind::Number => Value::Number(0.0),
            ValueKind::Bool => Value::Bool(false),
        }
    }
}

impl Value {
    pub fn kind(&self) -> ValueKind {
        match self {
            Value::Text(_) => ValueKind::Text,
            Value::Number(_) => ValueKind::Number,
            Value::Bool(_) => ValueKind::Bool,
        }
    }

    /// Interpret the value as a number: text is parsed, booleans are 0/1,
    /// anything unparseable is `None`.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Value::Text(t) => t.trim().parse().ok(),
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::Text(t) => t.eq_ignore_ascii_case("true"),
        }
    }

    /// Convert to `kind`, used when assigning across types.
    pub fn coerce(&self, kind: ValueKind) -> Value {
        match kind {
            ValueKind::Text => Value::Text(self.to_string()),
            ValueKind::Number => Value::Number(self.as_number().unwrap_or(0.0)),
            ValueKind::Bool => Value::Bool(self.as_bool()),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Text(t) => f.write_str(t),
            // Whole numbers print without a trailing ".0".
            Value::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => write!(f, "{}", *n as i64),
            Value::Number(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_coerce() {
        assert_eq!(Value::Number(3.0).to_string(), "3");
        assert_eq!(Value::Number(2.5).to_string(), "2.5");
        assert_eq!(Value::Text(" 42 ".into()).coerce(ValueKind::Number), Value::Number(42.0));
        assert_eq!(Value::Bool(true).coerce(ValueKind::Text), Value::Text("true".into()));
        assert_eq!(Value::Text("TRUE".into()).coerce(ValueKind::Bool), Value::Bool(true));
    }
}
