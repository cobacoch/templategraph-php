#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ViewName(String);

impl ViewName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
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
        let v = ViewName::new("partials.header");
        assert_eq!(v.as_str(), "partials.header");
    }

    #[test]
    fn view_name_equality_is_value_based() {
        let a = ViewName::new("layouts.app");
        let b = ViewName::new(String::from("layouts.app"));
        assert_eq!(a, b);
    }
}
