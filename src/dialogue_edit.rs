//! Granular dialogue-tree editing operations.
//!
//! Pure mutation helpers on a `&mut Option<DialogueTree>`. Used by:
//! - `src/api/mobiles.rs`: REST endpoints (one per op).
//! - `src/script/dialogue.rs`: Rhai bindings exposed to the medit OLC editor.
//!
//! Validation is opinionated: every op enforces the same invariants that
//! `validate_tree` checks (root exists, choice targets resolve), so failed
//! edits never persist a broken tree.

use crate::types::{DialogueChoice, DialogueEffect, DialogueNode, DialogueTree};

#[derive(Debug, Clone)]
pub enum DialogueEditError {
    NoTree,
    NodeMissing(String),
    NodeExists(String),
    CannotRemoveRoot(String),
    /// (target_node, referencing_node)
    NodeReferenced(String, String),
    /// (index, len)
    ChoiceIndexOutOfRange(usize, usize),
    BrokenChoiceTarget(String),
    EmptyNodeName,
}

impl std::fmt::Display for DialogueEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoTree => write!(f, "mobile has no dialogue tree"),
            Self::NodeMissing(n) => write!(f, "node `{}` not found", n),
            Self::NodeExists(n) => write!(f, "node `{}` already exists", n),
            Self::CannotRemoveRoot(n) => write!(f, "cannot remove root node `{}`", n),
            Self::NodeReferenced(t, r) => {
                write!(f, "node `{}` is referenced by a choice on node `{}`", t, r)
            }
            Self::ChoiceIndexOutOfRange(i, len) => {
                write!(f, "choice index {} out of range (node has {} choices)", i, len)
            }
            Self::BrokenChoiceTarget(n) => {
                write!(f, "choice target points to missing node `{}`", n)
            }
            Self::EmptyNodeName => write!(f, "node name must not be empty"),
        }
    }
}

impl std::error::Error for DialogueEditError {}

pub type EditResult<T> = Result<T, DialogueEditError>;

pub fn ensure_tree(slot: &mut Option<DialogueTree>) -> EditResult<&mut DialogueTree> {
    slot.as_mut().ok_or(DialogueEditError::NoTree)
}

/// Initialize an empty tree with a single root node ("root") if none present.
/// No-op if a tree is already set.
pub fn ensure_initialized(slot: &mut Option<DialogueTree>, root_text: &str) {
    if slot.is_some() {
        return;
    }
    let mut nodes = std::collections::HashMap::new();
    nodes.insert(
        "root".to_string(),
        DialogueNode {
            text: root_text.to_string(),
            choices: vec![],
            on_enter: vec![],
            on_each_visit: vec![],
            on_exit: vec![],
        },
    );
    *slot = Some(DialogueTree {
        root_node: "root".into(),
        nodes,
    });
}

/// Replace the root node pointer. Errors if the named node is missing.
pub fn set_root(slot: &mut Option<DialogueTree>, node_name: &str) -> EditResult<()> {
    let tree = ensure_tree(slot)?;
    if !tree.nodes.contains_key(node_name) {
        return Err(DialogueEditError::NodeMissing(node_name.into()));
    }
    tree.root_node = node_name.into();
    Ok(())
}

pub fn add_node(
    slot: &mut Option<DialogueTree>,
    name: &str,
    node: DialogueNode,
) -> EditResult<()> {
    if name.trim().is_empty() {
        return Err(DialogueEditError::EmptyNodeName);
    }
    let tree = ensure_tree(slot)?;
    if tree.nodes.contains_key(name) {
        return Err(DialogueEditError::NodeExists(name.into()));
    }
    tree.nodes.insert(name.into(), node);
    Ok(())
}

/// Patch fields of an existing node. Each Option field replaces the
/// corresponding field when Some.
pub struct NodePatch {
    pub text: Option<String>,
    pub on_enter: Option<Vec<DialogueEffect>>,
    pub on_each_visit: Option<Vec<DialogueEffect>>,
    pub on_exit: Option<Vec<DialogueEffect>>,
}

pub fn update_node(
    slot: &mut Option<DialogueTree>,
    name: &str,
    patch: NodePatch,
) -> EditResult<()> {
    let tree = ensure_tree(slot)?;
    let node = tree
        .nodes
        .get_mut(name)
        .ok_or_else(|| DialogueEditError::NodeMissing(name.into()))?;
    if let Some(t) = patch.text {
        node.text = t;
    }
    if let Some(v) = patch.on_enter {
        node.on_enter = v;
    }
    if let Some(v) = patch.on_each_visit {
        node.on_each_visit = v;
    }
    if let Some(v) = patch.on_exit {
        node.on_exit = v;
    }
    Ok(())
}

pub fn remove_node(slot: &mut Option<DialogueTree>, name: &str) -> EditResult<()> {
    let tree = ensure_tree(slot)?;
    if tree.root_node == name {
        return Err(DialogueEditError::CannotRemoveRoot(name.into()));
    }
    if !tree.nodes.contains_key(name) {
        return Err(DialogueEditError::NodeMissing(name.into()));
    }
    // Refuse if any other node has a Goto pointing at this one.
    for (other_name, other) in &tree.nodes {
        if other_name == name {
            continue;
        }
        for choice in &other.choices {
            if let crate::types::DialogueTarget::Goto { node } = &choice.target {
                if node == name {
                    return Err(DialogueEditError::NodeReferenced(
                        name.into(),
                        other_name.clone(),
                    ));
                }
            }
        }
    }
    tree.nodes.remove(name);
    Ok(())
}

pub fn add_choice(
    slot: &mut Option<DialogueTree>,
    node_name: &str,
    choice: DialogueChoice,
) -> EditResult<()> {
    let tree = ensure_tree(slot)?;
    validate_choice_target(tree, &choice)?;
    let node = tree
        .nodes
        .get_mut(node_name)
        .ok_or_else(|| DialogueEditError::NodeMissing(node_name.into()))?;
    node.choices.push(choice);
    Ok(())
}

pub fn update_choice(
    slot: &mut Option<DialogueTree>,
    node_name: &str,
    index: usize,
    choice: DialogueChoice,
) -> EditResult<()> {
    let tree = ensure_tree(slot)?;
    validate_choice_target(tree, &choice)?;
    let node = tree
        .nodes
        .get_mut(node_name)
        .ok_or_else(|| DialogueEditError::NodeMissing(node_name.into()))?;
    if index >= node.choices.len() {
        return Err(DialogueEditError::ChoiceIndexOutOfRange(
            index,
            node.choices.len(),
        ));
    }
    node.choices[index] = choice;
    Ok(())
}

pub fn remove_choice(
    slot: &mut Option<DialogueTree>,
    node_name: &str,
    index: usize,
) -> EditResult<()> {
    let tree = ensure_tree(slot)?;
    let node = tree
        .nodes
        .get_mut(node_name)
        .ok_or_else(|| DialogueEditError::NodeMissing(node_name.into()))?;
    if index >= node.choices.len() {
        return Err(DialogueEditError::ChoiceIndexOutOfRange(
            index,
            node.choices.len(),
        ));
    }
    node.choices.remove(index);
    Ok(())
}

fn validate_choice_target(tree: &DialogueTree, choice: &DialogueChoice) -> EditResult<()> {
    if let crate::types::DialogueTarget::Goto { node } = &choice.target {
        if !tree.nodes.contains_key(node) {
            return Err(DialogueEditError::BrokenChoiceTarget(node.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DialogueChoice, DialogueTarget};

    fn empty_node(text: &str) -> DialogueNode {
        DialogueNode {
            text: text.into(),
            choices: vec![],
            on_enter: vec![],
            on_each_visit: vec![],
            on_exit: vec![],
        }
    }

    #[test]
    fn ensure_initialized_creates_root() {
        let mut slot = None;
        ensure_initialized(&mut slot, "Hello.");
        let t = slot.unwrap();
        assert_eq!(t.root_node, "root");
        assert!(t.nodes.contains_key("root"));
    }

    #[test]
    fn add_node_rejects_duplicate() {
        let mut slot = None;
        ensure_initialized(&mut slot, "Hi.");
        assert!(add_node(&mut slot, "shop", empty_node("Wares.")).is_ok());
        let err = add_node(&mut slot, "shop", empty_node("Other.")).unwrap_err();
        assert!(matches!(err, DialogueEditError::NodeExists(_)));
    }

    #[test]
    fn remove_node_blocks_referenced_target() {
        let mut slot = None;
        ensure_initialized(&mut slot, "Hi.");
        add_node(&mut slot, "shop", empty_node("Wares.")).unwrap();
        let go_to_shop = DialogueChoice {
            keyword: "shop".into(),
            label: "shop".into(),
            target: DialogueTarget::Goto { node: "shop".into() },
            conditions: vec![],
            effects: vec![],
        };
        add_choice(&mut slot, "root", go_to_shop).unwrap();
        let err = remove_node(&mut slot, "shop").unwrap_err();
        assert!(matches!(err, DialogueEditError::NodeReferenced(_, _)));
    }

    #[test]
    fn remove_node_rejects_root() {
        let mut slot = None;
        ensure_initialized(&mut slot, "Hi.");
        let err = remove_node(&mut slot, "root").unwrap_err();
        assert!(matches!(err, DialogueEditError::CannotRemoveRoot(_)));
    }

    #[test]
    fn add_choice_validates_target() {
        let mut slot = None;
        ensure_initialized(&mut slot, "Hi.");
        let bad = DialogueChoice {
            keyword: "ghost".into(),
            label: "ghost".into(),
            target: DialogueTarget::Goto {
                node: "ghost".into(),
            },
            conditions: vec![],
            effects: vec![],
        };
        assert!(matches!(
            add_choice(&mut slot, "root", bad).unwrap_err(),
            DialogueEditError::BrokenChoiceTarget(_)
        ));
    }

    #[test]
    fn remove_choice_pops_index() {
        let mut slot = None;
        ensure_initialized(&mut slot, "Hi.");
        add_node(&mut slot, "a", empty_node("A.")).unwrap();
        add_node(&mut slot, "b", empty_node("B.")).unwrap();
        let to_a = DialogueChoice {
            keyword: "a".into(),
            label: "a".into(),
            target: DialogueTarget::Goto { node: "a".into() },
            conditions: vec![],
            effects: vec![],
        };
        let to_b = DialogueChoice {
            keyword: "b".into(),
            label: "b".into(),
            target: DialogueTarget::Goto { node: "b".into() },
            conditions: vec![],
            effects: vec![],
        };
        add_choice(&mut slot, "root", to_a).unwrap();
        add_choice(&mut slot, "root", to_b).unwrap();
        remove_choice(&mut slot, "root", 0).unwrap();
        let tree = slot.as_ref().unwrap();
        assert_eq!(tree.nodes["root"].choices.len(), 1);
        assert_eq!(tree.nodes["root"].choices[0].keyword, "b");
    }

    #[test]
    fn update_node_patches_only_some_fields() {
        let mut slot = None;
        ensure_initialized(&mut slot, "Hi.");
        update_node(
            &mut slot,
            "root",
            NodePatch {
                text: Some("New text.".into()),
                on_enter: None,
                on_each_visit: None,
                on_exit: None,
            },
        )
        .unwrap();
        let tree = slot.as_ref().unwrap();
        assert_eq!(tree.nodes["root"].text, "New text.");
    }
}
