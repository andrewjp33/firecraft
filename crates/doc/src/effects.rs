//! An object's Live Effects list as stored by Fireworks (`EFLv` > `EPSv` per effect > `DCEv`
//! key/value pairs), with what FireCraft renders for each.
//!
//! The stored parameters are the source of truth: editing changes them and re-parses the
//! rendered effect, and saving writes them back in their original order, so effects FireCraft
//! doesn't render (or render yet) survive untouched.

use std::collections::HashMap;

use firecraft_fwpng::{Kind, Node};
use firecraft_raster::effects::Effect;

/// A stored parameter value: text (`DCVs`), or a vector node kept as read (Curves points).
#[derive(Clone, Debug, PartialEq)]
pub enum EffectParam {
    Text(String),
    Node(Node),
}

/// One effect in an object's list.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectEntry {
    /// (key, value) in stored order.
    pub params: Vec<(String, EffectParam)>,
    /// What FireCraft renders for it (`None`: kept but not rendered).
    pub effect: Option<Effect>,
}

impl EffectEntry {
    pub fn new(params: Vec<(String, EffectParam)>) -> Self {
        let mut e = EffectEntry { params, effect: None };
        e.reparse();
        e
    }

    /// From text pairs (new effects).
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self::new(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), EffectParam::Text(v.to_string())))
                .collect(),
        )
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| match v {
                EffectParam::Text(t) => Some(t.as_str()),
                EffectParam::Node(_) => None,
            })
    }

    /// Set a text parameter (appended if new) and re-parse.
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        let value = EffectParam::Text(value.into());
        match self.params.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => self.params.push((key.to_owned(), value)),
        }
        self.reparse();
    }

    /// The checkbox in the effects list (`EffectIsVisible`).
    pub fn visible(&self) -> bool {
        self.get("EffectIsVisible") != Some("false")
    }

    pub fn set_visible(&mut self, on: bool) {
        self.set("EffectIsVisible", if on { "true" } else { "false" });
    }

    pub fn moa_id(&self) -> &str {
        self.get("EffectMoaID").unwrap_or("")
    }

    /// The name shown in the list ("Gaussian Blur...", "Drop Shadow").
    pub fn name(&self) -> String {
        self.get("name")
            .or_else(|| self.get("mkbFile_WriteOnly_TemporaryEffectUiName"))
            .map(str::to_owned)
            .unwrap_or_else(|| "Effect".to_owned())
    }

    /// Whether FireCraft renders it (shown and understood).
    pub fn active(&self) -> Option<&Effect> {
        self.effect.as_ref().filter(|_| self.visible())
    }

    /// Recompute the rendered effect from the parameters.
    pub fn reparse(&mut self) {
        let mut text = HashMap::new();
        let mut vectors = HashMap::new();
        for (k, v) in &self.params {
            match v {
                EffectParam::Text(t) => {
                    text.insert(k.clone(), t.clone());
                }
                EffectParam::Node(list) => {
                    let pts = list
                        .children
                        .iter()
                        .map(|p| {
                            (
                                p.float_of("XLC").unwrap_or(0.0) as f32,
                                p.float_of("YLC").unwrap_or(0.0) as f32,
                            )
                        })
                        .collect();
                    vectors.insert(k.clone(), pts);
                }
            }
        }
        self.effect = Effect::parse(&text, &vectors);
    }

    /// From an `EPSv` node.
    pub fn read(eps: &Node) -> Self {
        let params = eps
            .children_tagged("DCE")
            .filter_map(|dce| {
                let key = dce.str_of("DCK")?.to_owned();
                if let Some(v) = dce.str_of("DCV") {
                    return Some((key, EffectParam::Text(v.to_owned())));
                }
                let list = dce.children.iter().find(|c| c.kind == Kind::Vector)?;
                Some((key, EffectParam::Node(list.clone())))
            })
            .collect();
        Self::new(params)
    }

    /// As an `EPSv` node.
    pub fn write(&self) -> Node {
        Node::vector(
            "EPS",
            self.params
                .iter()
                .map(|(k, v)| {
                    let value = match v {
                        EffectParam::Text(t) => Node::string("DCV", t),
                        EffectParam::Node(n) => n.clone(),
                    };
                    Node::vector("DCE", vec![Node::string("DCK", k), value])
                })
                .collect(),
        )
    }
}

/// An `EFLv` node's effects.
pub fn read_list(efl: &Node) -> Vec<EffectEntry> {
    efl.children_tagged("EPS").map(EffectEntry::read).collect()
}

/// An `EFLv` node for `list`, keeping `previous`'s other children (`CATs`/`INMs`, the list's
/// preset name) when there was one.
pub fn write_list(list: &[EffectEntry], previous: Option<&Node>) -> Node {
    let mut children: Vec<Node> = match previous {
        Some(p) => p.children.iter().filter(|c| !c.is("EPS")).cloned().collect(),
        None => vec![Node::string("CAT", "UNUSED"), Node::string("INM", "UNUSED")],
    };
    children.extend(list.iter().map(EffectEntry::write));
    Node::vector("EFL", children)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_edits() {
        let mut e = EffectEntry::from_pairs(&[
            ("EffectIsVisible", "true"),
            ("EffectMoaID", "{d04ef8c0-71b3-11d1-8c8200a024cdc039}"),
            ("gaussian_blur_radius", "4"),
            ("name", "Gaussian Blur..."),
        ]);
        assert!(matches!(e.active(), Some(Effect::GaussianBlur { radius }) if *radius == 4.0));
        let back = EffectEntry::read(&e.write());
        assert_eq!(back, e);
        e.set("gaussian_blur_radius", "10");
        assert!(matches!(e.effect, Some(Effect::GaussianBlur { radius }) if radius == 10.0));
        e.set_visible(false);
        assert!(e.active().is_none());
        assert_eq!(e.name(), "Gaussian Blur...");
        // A list keeps its preset name and order.
        let efl = write_list(&[e.clone()], None);
        assert_eq!(read_list(&efl), vec![e]);
    }
}
