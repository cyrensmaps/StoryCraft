/// Replace `%Name%` references with values from `lookup`.
///
/// References to unknown names are left untouched, as is any `%` that does
/// not start a well-formed reference, so "50% off" survives intact.
pub fn substitute_variables(text: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let reference = after.find('%').map(|end| &after[..end]).filter(|name| is_name(name));
        match reference.and_then(|name| lookup(name).map(|v| (name, v))) {
            Some((name, value)) => {
                out.push_str(&value);
                rest = &after[name.len() + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(name: &str) -> Option<String> {
        match name {
            "Score" => Some("42".into()),
            "Name" => Some("Ada".into()),
            _ => None,
        }
    }

    #[test]
    fn substitutes_known_names() {
        assert_eq!(substitute_variables("Hi %Name%, score %Score%!", lookup), "Hi Ada, score 42!");
        assert_eq!(substitute_variables("%Score%%Score%", lookup), "4242");
    }

    #[test]
    fn leaves_other_percent_signs_alone() {
        assert_eq!(substitute_variables("50% off", lookup), "50% off");
        assert_eq!(substitute_variables("%Unknown% %Score%", lookup), "%Unknown% 42");
        assert_eq!(substitute_variables("100% and %Score%", lookup), "100% and 42");
        assert_eq!(substitute_variables("%%", lookup), "%%");
        assert_eq!(substitute_variables("trailing %", lookup), "trailing %");
    }
}
