use std::collections::{HashMap, HashSet};

use gpui::{AppContext, Context, Entity, SharedString, Subscription};

use crate::components::input::{InputEvent, TextInput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeNodeKind {
    Directory,
    File,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TreePickerMode {
    Files,
    #[default]
    Directories,
    Both,
}

#[derive(Clone, Debug)]
pub struct TreeNode {
    pub id: SharedString,
    pub parent_id: Option<SharedString>,
    pub label: SharedString,
    pub kind: TreeNodeKind,
    pub disabled: bool,
}

impl TreeNode {
    pub fn directory(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            parent_id: None,
            label: label.into(),
            kind: TreeNodeKind::Directory,
            disabled: false,
        }
    }

    pub fn file(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            parent_id: None,
            label: label.into(),
            kind: TreeNodeKind::File,
            disabled: false,
        }
    }

    pub fn parent(mut self, parent_id: impl Into<SharedString>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

pub struct TreePickerState {
    pub(crate) nodes: Vec<TreeNode>,
    pub(crate) mode: TreePickerMode,
    pub(crate) expanded: HashSet<SharedString>,
    pub(crate) selected: Option<SharedString>,
    pub(crate) query: String,
    pub(crate) loading: bool,
    pub(crate) search: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
}

impl TreePickerState {
    pub fn new(mode: TreePickerMode, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| TextInput::new(cx).placeholder("Search"));
        let subscription = cx.subscribe(&search, |state, _, event: &InputEvent, cx| {
            state.query = event.text().trim().to_lowercase();
            cx.notify();
        });
        Self {
            nodes: Vec::new(),
            mode,
            expanded: HashSet::new(),
            selected: None,
            query: String::new(),
            loading: false,
            search,
            _subscriptions: vec![subscription],
        }
    }

    pub fn set_nodes(&mut self, nodes: Vec<TreeNode>, cx: &mut Context<Self>) {
        let ids = nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        self.expanded.retain(|id| ids.contains(id));
        if self.selected.as_ref().is_some_and(|id| !ids.contains(id)) {
            self.selected = None;
        }
        self.nodes = nodes;
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: TreePickerMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.selected = None;
        cx.notify();
    }

    pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.loading = loading;
        cx.notify();
    }

    pub fn set_search_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.search
            .update(cx, |input, _| input.set_placeholder(placeholder));
    }

    pub fn selected(&self) -> Option<&TreeNode> {
        let id = self.selected.as_ref()?;
        self.nodes.iter().find(|node| &node.id == id)
    }

    pub fn selected_path(&self, separator: &str) -> Option<String> {
        let selected = self.selected()?;
        let nodes = self
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node))
            .collect::<HashMap<_, _>>();
        let mut labels = vec![selected.label.to_string()];
        let mut parent = selected.parent_id.clone();
        while let Some(id) = parent {
            let Some(node) = nodes.get(&id) else {
                break;
            };
            labels.push(node.label.to_string());
            parent = node.parent_id.clone();
        }
        labels.reverse();
        Some(labels.join(separator))
    }

    pub fn select(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(node) = self
            .nodes
            .iter()
            .find(|node| node.id.as_ref() == id && self.selectable(node))
        else {
            return false;
        };
        let selected = node.id.clone();
        let mut parent = node.parent_id.clone();
        let parents = self
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node.parent_id.clone()))
            .collect::<HashMap<_, _>>();
        while let Some(id) = parent {
            self.expanded.insert(id.clone());
            parent = parents.get(&id).cloned().flatten();
        }
        self.selected = Some(selected);
        cx.notify();
        true
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.query.clear();
        self.search.update(cx, |search, cx| search.clear(cx));
        cx.notify();
    }

    pub(crate) fn selectable(&self, node: &TreeNode) -> bool {
        !node.disabled
            && match self.mode {
                TreePickerMode::Files => node.kind == TreeNodeKind::File,
                TreePickerMode::Directories => node.kind == TreeNodeKind::Directory,
                TreePickerMode::Both => true,
            }
    }

    pub(crate) fn visible_nodes(&self) -> Vec<(TreeNode, usize, bool)> {
        let by_parent = self.nodes.iter().fold(
            HashMap::<Option<SharedString>, Vec<&TreeNode>>::new(),
            |mut groups, node| {
                groups.entry(node.parent_id.clone()).or_default().push(node);
                groups
            },
        );
        let known = self
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        let roots = self
            .nodes
            .iter()
            .filter(|node| node.parent_id.as_ref().is_none_or(|id| !known.contains(id)))
            .collect::<Vec<_>>();
        let matches = self.search_matches();
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        for root in roots {
            self.flatten(root, 0, &by_parent, &matches, &mut visited, &mut result);
        }
        result
    }

    fn search_matches(&self) -> Option<HashSet<SharedString>> {
        if self.query.is_empty() {
            return None;
        }
        let parents = self
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node.parent_id.clone()))
            .collect::<HashMap<_, _>>();
        let mut visible = HashSet::new();
        for node in self
            .nodes
            .iter()
            .filter(|node| node.label.to_lowercase().contains(&self.query))
        {
            visible.insert(node.id.clone());
            let mut parent = node.parent_id.clone();
            while let Some(id) = parent {
                if !visible.insert(id.clone()) {
                    break;
                }
                parent = parents.get(&id).cloned().flatten();
            }
        }
        Some(visible)
    }

    fn flatten(
        &self,
        node: &TreeNode,
        depth: usize,
        children: &HashMap<Option<SharedString>, Vec<&TreeNode>>,
        matches: &Option<HashSet<SharedString>>,
        visited: &mut HashSet<SharedString>,
        result: &mut Vec<(TreeNode, usize, bool)>,
    ) {
        if !visited.insert(node.id.clone())
            || matches.as_ref().is_some_and(|ids| !ids.contains(&node.id))
            || (self.mode == TreePickerMode::Directories && node.kind == TreeNodeKind::File)
        {
            return;
        }
        let has_children = children.get(&Some(node.id.clone())).is_some_and(|items| {
            items.iter().any(|child| {
                self.mode != TreePickerMode::Directories || child.kind == TreeNodeKind::Directory
            })
        });
        result.push((node.clone(), depth, has_children));
        let open = matches.is_some() || self.expanded.contains(&node.id);
        if open && has_children {
            for child in children.get(&Some(node.id.clone())).into_iter().flatten() {
                self.flatten(child, depth + 1, children, matches, visited, result);
            }
        }
    }
}
