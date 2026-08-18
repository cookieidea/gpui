use std::rc::Rc;

use gpui::{
    AnyElement, App, IntoElement, Pixels, SharedString, StyleRefinement, Styled, Window, px, rgb,
};

pub(crate) type MenuAction = Rc<dyn Fn(&mut Window, &mut App)>;
pub(crate) type MenuSlot = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

#[derive(Clone)]
pub(crate) enum ContextMenuItemKind {
    Action(MenuAction),
    Submenu(Box<ContextMenu>),
}

/// An actionable row or submenu entry.
#[derive(Clone)]
pub struct ContextMenuItem {
    pub(crate) label: MenuSlot,
    pub(crate) shortcut: Option<SharedString>,
    pub(crate) kind: ContextMenuItemKind,
    pub(crate) disabled: bool,
    pub(crate) danger: bool,
    pub(crate) keep_open: bool,
}

impl ContextMenuItem {
    fn slot<E: IntoElement>(render: impl Fn(&mut Window, &mut App) -> E + 'static) -> MenuSlot {
        Rc::new(move |window, cx| render(window, cx).into_any_element())
    }

    pub fn action(
        label: impl Into<SharedString>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let label = label.into();
        Self::action_with(move |_, _| label.clone(), action)
    }

    pub fn action_with<E: IntoElement>(
        label: impl Fn(&mut Window, &mut App) -> E + 'static,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label: Self::slot(label),
            shortcut: None,
            kind: ContextMenuItemKind::Action(Rc::new(action)),
            disabled: false,
            danger: false,
            keep_open: false,
        }
    }

    pub(crate) fn submenu(label: SharedString, submenu: ContextMenu) -> Self {
        Self::submenu_with(move |_, _| label.clone(), submenu)
    }

    pub fn submenu_with<E: IntoElement>(
        label: impl Fn(&mut Window, &mut App) -> E + 'static,
        submenu: ContextMenu,
    ) -> Self {
        Self {
            label: Self::slot(label),
            shortcut: None,
            kind: ContextMenuItemKind::Submenu(Box::new(submenu)),
            disabled: false,
            danger: false,
            keep_open: false,
        }
    }

    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// Keeps the menu open after this action runs, useful for toggles and check items.
    pub fn keep_open(mut self, keep_open: bool) -> Self {
        self.keep_open = keep_open;
        self
    }
}

#[derive(Clone)]
pub(crate) enum ContextMenuEntry {
    Item(ContextMenuItem),
    Separator,
}

/// Semantic contents and session-wide presentation settings for a context menu.
#[derive(Clone)]
pub struct ContextMenu {
    pub(crate) entries: Vec<ContextMenuEntry>,
    pub(crate) appearance: Option<super::ContextMenuAppearance>,
    pub(crate) surfaces: super::ContextMenuSurfaces,
    pub(crate) style: StyleRefinement,
    pub(crate) viewport_margin: Pixels,
    pub(crate) submenu_gap: Pixels,
}

impl Default for ContextMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextMenu {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            appearance: None,
            surfaces: super::ContextMenuSurfaces::default(),
            style: StyleRefinement::default()
                .w(px(220.0))
                .max_h(px(420.0))
                .p(px(5.0))
                .rounded(px(10.0))
                .border(px(1.0))
                .border_color(rgb(0x000000).opacity(0.12))
                .bg(rgb(0xffffff))
                .text_color(rgb(0x172033)),
            viewport_margin: px(8.0),
            submenu_gap: px(2.0),
        }
    }

    pub fn item(mut self, item: ContextMenuItem) -> Self {
        self.entries.push(ContextMenuEntry::Item(item));
        self
    }

    pub fn action(
        self,
        label: impl Into<SharedString>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.item(ContextMenuItem::action(label, action))
    }

    pub fn action_with<E: IntoElement>(
        self,
        label: impl Fn(&mut Window, &mut App) -> E + 'static,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.item(ContextMenuItem::action_with(label, action))
    }

    pub fn action_with_shortcut(
        self,
        label: impl Into<SharedString>,
        shortcut: impl Into<SharedString>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.item(ContextMenuItem::action(label, action).shortcut(shortcut))
    }

    /// Adds a submenu. Nesting is validated when the root menu is shown.
    pub fn submenu(
        self,
        label: impl Into<SharedString>,
        build: impl FnOnce(ContextMenu) -> ContextMenu,
    ) -> Self {
        self.item(ContextMenuItem::submenu(
            label.into(),
            build(ContextMenu::new()),
        ))
    }

    /// Adds a submenu with arbitrary GPUI content in its label area.
    pub fn submenu_with<E: IntoElement>(
        self,
        label: impl Fn(&mut Window, &mut App) -> E + 'static,
        build: impl FnOnce(ContextMenu) -> ContextMenu,
    ) -> Self {
        self.item(ContextMenuItem::submenu_with(
            label,
            build(ContextMenu::new()),
        ))
    }

    pub fn separator(mut self) -> Self {
        self.entries.push(ContextMenuEntry::Separator);
        self
    }

    pub fn appearance(mut self, appearance: super::ContextMenuAppearance) -> Self {
        self.appearance = Some(appearance);
        self
    }

    /// Sets the minimum distance kept between a menu and the window edge.
    pub fn viewport_margin(mut self, margin: Pixels) -> Self {
        self.viewport_margin = margin.max(px(0.0));
        self
    }

    /// Sets the visual gap between a parent menu and its submenu.
    pub fn submenu_gap(mut self, gap: Pixels) -> Self {
        self.submenu_gap = gap.max(px(0.0));
        self
    }

    /// Uses the same custom surface for root and submenu levels.
    pub fn surface<E: IntoElement>(
        mut self,
        render: impl Fn(super::ContextMenuSurfaceState, AnyElement, &mut Window, &mut App) -> E
        + 'static,
    ) -> Self {
        let surface = super::ContextMenuSurface::new(render);
        self.surfaces = super::ContextMenuSurfaces::all(surface);
        self
    }

    /// Sets only the root menu surface.
    pub fn root_surface<E: IntoElement>(
        mut self,
        render: impl Fn(super::ContextMenuSurfaceState, AnyElement, &mut Window, &mut App) -> E
        + 'static,
    ) -> Self {
        self.surfaces.by_depth[0] = Some(super::ContextMenuSurface::new(render));
        self
    }

    /// Sets the surface inherited by both submenu levels.
    pub fn submenu_surface<E: IntoElement>(
        mut self,
        render: impl Fn(super::ContextMenuSurfaceState, AnyElement, &mut Window, &mut App) -> E
        + 'static,
    ) -> Self {
        let surface = super::ContextMenuSurface::new(render);
        self.surfaces.by_depth[1] = Some(surface.clone());
        self.surfaces.by_depth[2] = Some(surface);
        self
    }

    /// Sets a surface for a precise zero-based depth: 0=root, 1=second, 2=third.
    pub fn surface_for_depth<E: IntoElement>(
        mut self,
        depth: usize,
        render: impl Fn(super::ContextMenuSurfaceState, AnyElement, &mut Window, &mut App) -> E
        + 'static,
    ) -> Self {
        assert!(
            depth < super::MAX_CONTEXT_MENU_DEPTH,
            "context menu surface depth must be 0, 1, or 2"
        );
        self.surfaces.by_depth[depth] = Some(super::ContextMenuSurface::new(render));
        self
    }

    pub(crate) fn validate_depth(&self, depth: usize) -> Result<(), usize> {
        if depth >= super::MAX_CONTEXT_MENU_DEPTH {
            return Err(depth + 1);
        }
        for entry in &self.entries {
            if let ContextMenuEntry::Item(ContextMenuItem {
                kind: ContextMenuItemKind::Submenu(submenu),
                ..
            }) = entry
            {
                submenu.validate_depth(depth + 1)?;
            }
        }
        Ok(())
    }
}

impl Styled for ContextMenu {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[cfg(test)]
mod tests {
    use gpui::{FontWeight, px};

    use super::*;

    #[test]
    fn common_visual_properties_use_styled() {
        let menu = ContextMenu::new()
            .font_family("UIC Test Family")
            .font_weight(FontWeight::MEDIUM)
            .text_size(px(15.0))
            .line_height(px(22.0))
            .opacity(0.8);

        assert_eq!(menu.style.opacity, Some(0.8));
        assert!(menu.style.text.font_family.is_some());
        assert_eq!(menu.style.text.font_weight, Some(FontWeight::MEDIUM));
        assert!(menu.style.text.font_size.is_some());
        assert!(menu.style.text.line_height.is_some());
    }

    #[test]
    fn accepts_three_levels() {
        let menu = ContextMenu::new().submenu("Second", |menu| {
            menu.submenu("Third", |menu| menu.action("Action", |_, _| {}))
        });

        assert_eq!(menu.validate_depth(0), Ok(()));
    }

    #[test]
    fn rejects_a_fourth_level() {
        let menu = ContextMenu::new().submenu("Second", |menu| {
            menu.submenu("Third", |menu| {
                menu.submenu("Fourth", |menu| menu.action("Action", |_, _| {}))
            })
        });

        assert_eq!(menu.validate_depth(0), Err(4));
    }
}
