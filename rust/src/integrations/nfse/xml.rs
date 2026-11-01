//! Just enough XML to write the DPS and its signature: elements with a default
//! namespace, attributes and text, serialized plainly or in canonical form
//! (C14N 1.0) for digesting.

#[derive(Debug, Clone)]
pub enum Node {
    El(El),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct El {
    pub name: String,
    /// Default namespace declared on this element.
    pub ns: Option<String>,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

fn escape_text(s: &str, canonical: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' if canonical => out.push_str("&#xD;"),
            c => out.push(c),
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#x9;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            c => out.push(c),
        }
    }
    out
}

impl El {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ns: None,
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }

    /// `<name>text</name>`
    pub fn leaf(name: &str, text: impl Into<String>) -> Self {
        Self::new(name).text(text)
    }

    pub fn ns(mut self, ns: &str) -> Self {
        self.ns = Some(ns.to_string());
        self
    }

    pub fn attr(mut self, name: &str, value: impl Into<String>) -> Self {
        self.attrs.push((name.to_string(), value.into()));
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.children.push(Node::Text(text.into()));
        self
    }

    pub fn child(mut self, el: El) -> Self {
        self.children.push(Node::El(el));
        self
    }

    pub fn maybe(self, el: Option<El>) -> Self {
        match el {
            Some(el) => self.child(el),
            None => self,
        }
    }

    pub fn find(&self, name: &str) -> Option<&El> {
        self.children.iter().find_map(|n| match n {
            Node::El(el) if el.name == name => Some(el),
            _ => None,
        })
    }

    fn write(&self, out: &mut String, canonical: bool, inherited: Option<&str>) {
        out.push('<');
        out.push_str(&self.name);
        if let Some(ns) = self.ns.as_deref().or(inherited) {
            out.push_str(&format!(" xmlns=\"{}\"", escape_attr(ns)));
        }
        let mut attrs: Vec<&(String, String)> = self.attrs.iter().collect();
        if canonical {
            attrs.sort_by(|a, b| a.0.cmp(&b.0));
        }
        for (name, value) in attrs {
            out.push_str(&format!(" {name}=\"{}\"", escape_attr(value)));
        }
        if self.children.is_empty() && !canonical {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for child in &self.children {
            match child {
                Node::El(el) => el.write(out, canonical, None),
                Node::Text(t) => out.push_str(&escape_text(t, canonical)),
            }
        }
        out.push_str(&format!("</{}>", self.name));
    }

    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, false, None);
        out
    }

    /// Canonical form of this element on its own; `inherited` is the default
    /// namespace in scope from its ancestors, which C14N repeats on it.
    pub fn canonical(&self, inherited: Option<&str>) -> String {
        let mut out = String::new();
        self.write(&mut out, true, inherited);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_plain_and_canonical_forms() {
        let el = El::new("a")
            .attr("z", "1")
            .attr("Id", "x\"y")
            .child(El::leaf("b", "R&D <1>"))
            .child(El::new("c").attr("k", "v"));
        assert_eq!(
            el.to_xml(),
            r#"<a z="1" Id="x&quot;y"><b>R&amp;D &lt;1&gt;</b><c k="v"/></a>"#
        );
        assert_eq!(
            el.canonical(Some("urn:x")),
            r#"<a xmlns="urn:x" Id="x&quot;y" z="1"><b>R&amp;D &lt;1&gt;</b><c k="v"></c></a>"#
        );
    }
}
