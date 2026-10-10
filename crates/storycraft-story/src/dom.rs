//! A minimal XML tree, which is easier to map from than a pull parser.

use quick_xml::events::Event;

#[derive(Debug, Default, Clone)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// Attribute value, or "" when absent.
    pub fn a(&self, name: &str) -> &str {
        self.attr(name).unwrap_or("")
    }

    pub fn attr_f32(&self, name: &str) -> Option<f32> {
        self.attr(name)?.trim().parse().ok().filter(|v: &f32| v.is_finite())
    }

    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// Follow a path of child names, e.g. `["bG", "solidFill", "clr"]`.
    pub fn path(&self, path: &[&str]) -> Option<&Node> {
        path.iter().try_fold(self, |node, name| node.child(name))
    }
}

pub fn parse(bytes: &[u8]) -> Result<Node, String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut stack: Vec<Node> = vec![Node::default()];
    let mut buf = Vec::new();
    loop {
        let event = reader.read_event_into(&mut buf).map_err(|e| e.to_string())?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let mut node = Node { name: e.name().as_ref().to_owned(), ..Default::default() };
                for attr in e.attributes().flatten() {
                    let value = attr.normalized_value(quick_xml::XmlVersion::default()).map_err(|e| e.to_string())?;
                    node.attrs.push((attr.key.as_ref().to_owned(), value.into_owned()));
                }
                if matches!(event, Event::Empty(_)) {
                    stack.last_mut().unwrap().children.push(node);
                } else {
                    stack.push(node);
                }
            }
            Event::End(_) => {
                let node = stack.pop().ok_or("unbalanced XML")?;
                stack.last_mut().ok_or("unbalanced XML")?.children.push(node);
            }
            Event::Text(t) => stack.last_mut().unwrap().text.push_str(&t.xml10_content()),
            Event::CData(t) => stack.last_mut().unwrap().text.push_str(&t),
            Event::GeneralRef(r) => {
                let resolved = match r.resolve_char_ref().map_err(|e| e.to_string())? {
                    Some(c) => c,
                    None => match &*r.xml10_content() {
                        "lt" => '<',
                        "gt" => '>',
                        "amp" => '&',
                        "quot" => '"',
                        "apos" => '\'',
                        other => return Err(format!("unknown entity &{other};")),
                    },
                };
                stack.last_mut().unwrap().text.push(resolved);
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    let mut root = stack.pop().ok_or("empty document")?;
    if !stack.is_empty() {
        return Err("unclosed elements".into());
    }
    root.children.pop().ok_or_else(|| "no root element".to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_entities_and_nesting() {
        let root = super::parse(br#"<a x="1 &amp; 2"><b>&lt;Doc&gt;&#65;</b><c/></a>"#).unwrap();
        assert_eq!(root.name, "a");
        assert_eq!(root.a("x"), "1 & 2");
        assert_eq!(root.child("b").unwrap().text, "<Doc>A");
        assert!(root.path(&["c"]).is_some());
    }
}
