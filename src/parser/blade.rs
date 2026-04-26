use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ViewNameError {
    #[error("view name is empty or whitespace only")]
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ViewName(String);

impl ViewName {
    pub fn new(name: impl Into<String>) -> Result<Self, ViewNameError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(ViewNameError::Empty);
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_name_round_trip() {
        let v = ViewName::new("partials.header").unwrap();
        assert_eq!(v.as_str(), "partials.header");
    }

    #[test]
    fn view_name_equality_is_value_based() {
        let a = ViewName::new("layouts.app").unwrap();
        let b = ViewName::new(String::from("layouts.app")).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn view_name_rejects_empty_string() {
        assert!(matches!(ViewName::new(""), Err(ViewNameError::Empty)));
    }

    #[test]
    fn view_name_rejects_whitespace_only() {
        assert!(matches!(ViewName::new("   "), Err(ViewNameError::Empty)));
        assert!(matches!(ViewName::new("\t\n"), Err(ViewNameError::Empty)));
    }
}
