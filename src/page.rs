use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use blitz_dom::{BaseDocument, DocumentConfig, Node, NodeId};
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};

use style::properties::ComputedValues;
use style::servo_arc::Arc as ServoArc;

use crate::net::FileNetProvider;
use crate::property::Property;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl std::fmt::Display for Size {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl std::str::FromStr for Size {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (w, h) = s
            .split_once(['x', 'X'])
            .ok_or_else(|| format!("viewport must be WIDTHxHEIGHT, got {s:?}"))?;
        let parse = |v: &str| {
            v.trim()
                .parse::<u32>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("invalid viewport dimension {v:?} in {s:?}"))
        };
        Ok(Size {
            width: parse(w)?,
            height: parse(h)?,
        })
    }
}

pub struct Page {
    pub doc: BaseDocument,
    pub warnings: Vec<String>,
    fetch_warnings: Arc<std::sync::Mutex<Vec<String>>>,
}

/// An element (or one of its `::before`/`::after` pseudo-elements) that has styles.
pub struct Entry {
    /// Structural path (`/html[1]/body[1]/div[2]`), used to pair elements across
    /// the two versions so that renamed classes still line up.
    pub key: String,
    /// Human-readable path with ids and classes.
    pub label: String,
    pub node: NodeId,
}

impl Page {
    /// Parse `root/relative` with `root` acting as the site root for `/…` URLs.
    pub fn load(root: &Path, base: &str, relative: &Path) -> Result<Page> {
        let path = root.join(relative);
        let html = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let url_path: Vec<String> = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let (provider, fetch_warnings) = FileNetProvider::new(root.to_path_buf(), base);
        let config = DocumentConfig {
            base_url: Some(format!(
                "file://{}/{}",
                base.trim_end_matches('/'),
                url_path.join("/")
            )),
            net_provider: Some(Arc::new(provider)),
            ..Default::default()
        };
        let doc = HtmlDocument::from_html(&html, config).into_inner();
        Ok(Page {
            doc,
            warnings: Vec::new(),
            fetch_warnings,
        })
    }

    pub fn resolve_at(&mut self, size: Size) {
        self.doc.set_viewport(Viewport::new(
            size.width,
            size.height,
            1.0,
            ColorScheme::Light,
        ));
        self.doc.resolve(0.0);
        self.warnings
            .extend(self.fetch_warnings.lock().unwrap().drain(..));
    }

    pub fn entries(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        walk(&self.doc, self.doc.root_node(), "", "", &mut out);
        out
    }

    /// The node's computed values. Elements inside a `display: none` subtree are
    /// skipped by the style traversal, so theirs are computed on demand.
    pub fn computed(&self, node: NodeId) -> Option<ServoArc<ComputedValues>> {
        match self.doc.get_node(node)?.primary_styles() {
            Some(styles) => Some(ServoArc::clone(&styles)),
            None => self.doc.resolve_undisplayed_style(node),
        }
    }

    /// The value `getComputedStyle()` would return, using layout for used values.
    pub fn used(&self, node: NodeId, property: Property) -> String {
        self.doc.resolved_style_value(node, property.name)
    }
}

fn walk(doc: &BaseDocument, parent: &Node, key: &str, label: &str, out: &mut Vec<Entry>) {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for &child_id in parent.children.iter() {
        let Some(child) = doc.get_node(child_id) else {
            continue;
        };
        let Some(data) = child.element_data() else {
            continue;
        };
        let tag: &str = &data.name.local;
        let index = match seen.iter_mut().find(|(t, _)| *t == tag) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                seen.push((tag, 1));
                1
            }
        };
        let child_key = format!("{key}/{tag}[{index}]");
        let mut step = tag.to_string();
        if let Some(id) = child.attr(blitz_dom::local_name!("id")) {
            step.push('#');
            step.push_str(id);
        }
        if let Some(class) = child.attr(blitz_dom::local_name!("class")) {
            for c in class.split_ascii_whitespace() {
                step.push('.');
                step.push_str(c);
            }
        }
        if index > 1 {
            step.push_str(&format!("[{index}]"));
        }
        let child_label = if label.is_empty() {
            step
        } else {
            format!("{label} > {step}")
        };

        out.push(Entry {
            key: child_key.clone(),
            label: child_label.clone(),
            node: child_id,
        });
        for (pseudo, id) in [("::before", child.before()), ("::after", child.after())] {
            if let Some(id) = id {
                out.push(Entry {
                    key: format!("{child_key}{pseudo}"),
                    label: format!("{child_label}{pseudo}"),
                    node: id,
                });
            }
        }
        walk(doc, child, &child_key, &child_label, out);
    }
}
