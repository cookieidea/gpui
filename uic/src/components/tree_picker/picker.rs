use gpui::{
    App, Entity, IntoElement, Refineable as _, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, div, prelude::*, svg,
};

use crate::components::input::Input;

use super::{TreeNodeKind, TreePickerAppearance, TreePickerState};

#[derive(IntoElement)]
pub struct TreePicker {
    state: Entity<TreePickerState>,
    appearance: TreePickerAppearance,
    searchable: bool,
    empty_text: SharedString,
    loading_text: SharedString,
    style: StyleRefinement,
}

impl TreePicker {
    pub fn new(state: &Entity<TreePickerState>) -> Self {
        Self {
            state: state.clone(),
            appearance: TreePickerAppearance::default(),
            searchable: true,
            empty_text: "No matching items".into(),
            loading_text: "Loading...".into(),
            style: StyleRefinement::default()
                .min_h(gpui::px(280.))
                .max_h(gpui::px(440.))
                .p_2()
                .rounded(gpui::px(10.))
                .border_1()
                .border_color(gpui::hsla(0., 0., 0.85, 1.))
                .bg(gpui::hsla(0., 0., 1., 1.))
                .text_color(gpui::hsla(0., 0., 0.08, 1.)),
        }
    }

    pub fn appearance(mut self, appearance: TreePickerAppearance) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn searchable(mut self, searchable: bool) -> Self {
        self.searchable = searchable;
        self
    }

    pub fn empty_text(mut self, text: impl Into<SharedString>) -> Self {
        self.empty_text = text.into();
        self
    }

    pub fn loading_text(mut self, text: impl Into<SharedString>) -> Self {
        self.loading_text = text.into();
        self
    }
}

impl RenderOnce for TreePicker {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let appearance = self.appearance.clone();
        let (rows, selected, search, loading) = {
            let state = self.state.read(cx);
            (
                state.visible_nodes(),
                state.selected.clone(),
                state.search.clone(),
                state.loading,
            )
        };
        let mut content = div().flex().flex_col().gap_2();
        if self.searchable {
            let search_input = Input::new(&search).appearance(appearance.search).prefix(
                svg()
                    .path(appearance.search_icon.clone())
                    .size_4()
                    .text_color(appearance.muted),
            );
            content = content.child(search_input);
        }
        let mut tree = div().id("tree-picker-scroll").overflow_y_scroll();
        tree.style().refine(&self.style);
        if loading {
            tree = tree.child(
                div()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(appearance.muted)
                    .child(self.loading_text),
            );
        } else if rows.is_empty() {
            tree = tree.child(
                div()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(appearance.muted)
                    .child(self.empty_text),
            );
        }
        for (node, depth, has_children) in rows.into_iter().filter(|_| !loading) {
            let node_id = node.id.clone();
            let expand_id = node.id.clone();
            let select_state = self.state.clone();
            let expand_state = self.state.clone();
            let is_selected = selected.as_ref() == Some(&node.id);
            let (selectable, expanded) = {
                let state = self.state.read(cx);
                (state.selectable(&node), state.expanded.contains(&node.id))
            };
            let icon = match node.kind {
                TreeNodeKind::Directory => appearance.directory_icon.clone(),
                TreeNodeKind::File => appearance.file_icon.clone(),
            };
            let row = div()
                .id(SharedString::from(format!("tree-picker-row-{}", node.id)))
                .h(appearance.row_height)
                .pl(appearance.indent * depth as f32)
                .pr_3()
                .flex()
                .items_center()
                .gap_2()
                .rounded_lg()
                .when(node.disabled, |this| this.text_color(appearance.muted))
                .when(is_selected, |this| this.bg(appearance.selected))
                .when(selectable, |this| {
                    this.cursor_pointer()
                        .hover(move |this| this.bg(appearance.hover))
                        .on_click(move |_, _, cx| {
                            select_state.update(cx, |state, cx| {
                                state.selected = Some(node_id.clone());
                                cx.notify();
                            });
                        })
                })
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "tree-picker-toggle-{}",
                            node.id
                        )))
                        .size_6()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(has_children, |this| {
                            this.cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    expand_state.update(cx, |state, cx| {
                                        if !state.expanded.remove(&expand_id) {
                                            state.expanded.insert(expand_id.clone());
                                        }
                                        cx.notify();
                                    });
                                    cx.stop_propagation();
                                })
                                .child(
                                    svg()
                                        .path(if expanded {
                                            appearance.expanded_icon.clone()
                                        } else {
                                            appearance.collapsed_icon.clone()
                                        })
                                        .size_4()
                                        .text_color(appearance.muted),
                                )
                        }),
                )
                .child(
                    div()
                        .size_8()
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_lg()
                        .bg(appearance.icon_background)
                        .child(svg().path(icon).size_4().text_color(appearance.accent)),
                )
                .child(div().flex_1().truncate().child(node.label))
                .when(is_selected, |this| {
                    this.child(
                        svg()
                            .path(appearance.selected_icon.clone())
                            .size_4()
                            .text_color(appearance.accent),
                    )
                });
            tree = tree.child(row);
        }
        content.child(tree)
    }
}

impl Styled for TreePicker {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
