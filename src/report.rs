use std::collections::BTreeMap;
use std::fmt::Write;

use serde::Serialize;

use crate::diff::{PageDiff, Side};

/// `(element, property, before, after, path)`
type ChangeKey = (String, String, String, String, String);

#[derive(Debug, Serialize)]
pub struct Report {
    pub viewports: Vec<String>,
    pub pages: Vec<PageReport>,
    pub only_before: Vec<String>,
    pub only_after: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PageReport {
    pub page: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<(u16, u16)>,
    pub elements: usize,
    pub changes: Vec<GroupedChange>,
    pub unmatched: Vec<GroupedUnmatched>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct GroupedChange {
    pub element: String,
    /// Structural path used to pair elements, e.g. `/html[1]/body[1]/p[2]`
    pub path: String,
    pub property: String,
    pub before: String,
    pub after: String,
    pub viewports: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct GroupedUnmatched {
    pub element: String,
    pub path: String,
    pub only_in: Side,
    pub viewports: Vec<String>,
}

impl Report {
    pub fn has_differences(&self) -> bool {
        !self.only_before.is_empty()
            || !self.only_after.is_empty()
            || self
                .pages
                .iter()
                .any(|p| p.status.is_some() || !p.changes.is_empty() || !p.unmatched.is_empty())
    }

    pub fn page(page: String, diff: PageDiff) -> PageReport {
        let mut changes: BTreeMap<ChangeKey, Vec<String>> = BTreeMap::new();
        let mut order: Vec<(usize, ChangeKey)> = Vec::new();
        for c in diff.changes {
            let key = (c.element, c.property, c.before, c.after, c.path);
            if !changes.contains_key(&key) {
                order.push((c.order, key.clone()));
            }
            changes.entry(key).or_default().push(c.viewport);
        }
        order.sort_by_key(|(position, _)| *position);
        let order: Vec<_> = order.into_iter().map(|(_, key)| key).collect();
        let mut unmatched: BTreeMap<(String, bool, String), Vec<String>> = BTreeMap::new();
        let mut unmatched_order = Vec::new();
        for u in diff.unmatched {
            let key = (u.element, u.only_in == Side::After, u.path);
            if !unmatched.contains_key(&key) {
                unmatched_order.push(key.clone());
            }
            unmatched.entry(key).or_default().push(u.viewport);
        }
        PageReport {
            page,
            status: diff.status,
            elements: diff.elements,
            changes: order
                .into_iter()
                .map(|key| {
                    let viewports = changes.remove(&key).unwrap();
                    let (element, property, before, after, path) = key;
                    GroupedChange {
                        element,
                        path,
                        property,
                        before,
                        after,
                        viewports,
                    }
                })
                .collect(),
            unmatched: unmatched_order
                .into_iter()
                .map(|key| {
                    let viewports = unmatched.remove(&key).unwrap();
                    GroupedUnmatched {
                        element: key.0,
                        path: key.2,
                        only_in: if key.1 { Side::After } else { Side::Before },
                        viewports,
                    }
                })
                .collect(),
            warnings: diff.warnings,
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let viewports = |v: &[String]| {
            let state = v[0].find(':').map_or("", |i| &v[0][i..]);
            let all = v.len() == self.viewports.len()
                && self
                    .viewports
                    .iter()
                    .all(|s| v.contains(&format!("{s}{state}")));
            match (all, state) {
                (true, "") => "all viewports".to_string(),
                (true, state) => format!("all viewports, {state} only"),
                _ => v.join(", "),
            }
        };
        for p in &self.only_before {
            writeln!(out, "- {p}: page only in before").unwrap();
        }
        for p in &self.only_after {
            writeln!(out, "+ {p}: page only in after").unwrap();
        }
        for page in &self.pages {
            if page.status.is_none() && page.changes.is_empty() && page.unmatched.is_empty() {
                continue;
            }
            writeln!(out, "{}", page.page).unwrap();
            if let Some((b, a)) = page.status {
                writeln!(out, "  HTTP status {b} → {a}").unwrap();
            }
            for u in &page.unmatched {
                let (sign, side) = match u.only_in {
                    Side::Before => ('-', "before"),
                    Side::After => ('+', "after"),
                };
                writeln!(
                    out,
                    "  {sign} {}  (only in {side}; {})",
                    u.element,
                    viewports(&u.viewports)
                )
                .unwrap();
            }
            let mut current = "";
            for c in &page.changes {
                if c.element != current {
                    writeln!(out, "  {}", c.element).unwrap();
                    current = &c.element;
                }
                writeln!(
                    out,
                    "    {}: {} → {}  ({})",
                    c.property,
                    show(&c.before),
                    show(&c.after),
                    viewports(&c.viewports)
                )
                .unwrap();
            }
        }
        out
    }
}

fn show(v: &str) -> &str {
    if v.is_empty() { "(none)" } else { v }
}
