use gpui::{
    AnyElement, App, CursorStyle, Entity, Focusable, IntoElement, MouseButton, Pixels,
    Refineable as _, RenderOnce, StyleRefinement, Styled, Window, deferred, div, prelude::*, px,
};

use super::{DropdownPlacement, DropdownState};

#[derive(IntoElement)]
pub struct Dropdown {
    state: Entity<DropdownState>,
    trigger: Option<AnyElement>,
    menu: Option<AnyElement>,
    placement: DropdownPlacement,
    menu_gap: Pixels,
    priority: usize,
    style: StyleRefinement,
}

pub fn dropdown(state: &Entity<DropdownState>) -> Dropdown {
    Dropdown::new(state)
}

impl Dropdown {
    pub fn new(state: &Entity<DropdownState>) -> Self {
        Self {
            state: state.clone(),
            trigger: None,
            menu: None,
            placement: DropdownPlacement::default(),
            menu_gap: px(6.),
            priority: 100,
            style: StyleRefinement::default()
                .min_w(px(160.))
                .max_h(px(320.))
                .p(px(6.))
                .rounded(px(8.))
                .border(px(1.))
                .border_color(gpui::black().opacity(0.12))
                .bg(gpui::white()),
        }
    }

    pub fn trigger(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }

    pub fn menu(mut self, menu: impl IntoElement) -> Self {
        self.menu = Some(menu.into_any_element());
        self
    }

    pub fn placement(mut self, placement: DropdownPlacement) -> Self {
        self.placement = placement;
        self
    }

    pub fn menu_gap(mut self, gap: Pixels) -> Self {
        self.menu_gap = gap.max(px(0.));
        self
    }

    pub fn priority(mut self, priority: usize) -> Self {
        self.priority = priority;
        self
    }
}

impl RenderOnce for Dropdown {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let open = self.state.read(cx).is_open();
        let state_id = self.state.entity_id();
        let focus_handle = self.state.focus_handle(cx);
        let toggle_state = self.state.clone();
        let escape_state = self.state.clone();
        let menu_style = self.style.clone();
        let menu_gap = self.menu_gap;

        let menu = open.then(|| {
            let positioned = match self.placement {
                DropdownPlacement::BottomStart => div().absolute().top_full().left_0(),
                DropdownPlacement::BottomEnd => div().absolute().top_full().right_0(),
                DropdownPlacement::TopStart => div().absolute().bottom_full().left_0(),
                DropdownPlacement::TopEnd => div().absolute().bottom_full().right_0(),
            };

            let mut positioned = positioned
                .id(("dropdown-menu", state_id))
                .mt(match self.placement {
                    DropdownPlacement::BottomStart | DropdownPlacement::BottomEnd => menu_gap,
                    DropdownPlacement::TopStart | DropdownPlacement::TopEnd => gpui::px(0.),
                })
                .mb(match self.placement {
                    DropdownPlacement::TopStart | DropdownPlacement::TopEnd => menu_gap,
                    DropdownPlacement::BottomStart | DropdownPlacement::BottomEnd => gpui::px(0.),
                })
                .overflow_y_scroll()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(self.menu);
            positioned.style().refine(&menu_style);
            deferred(positioned).with_priority(self.priority)
        });

        div()
            .relative()
            .track_focus(&focus_handle)
            .on_key_down(move |event, window, cx| {
                if event.keystroke.key == "escape" && escape_state.read(cx).is_open() {
                    escape_state.update(cx, |state, cx| state.close(window, cx));
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .cursor(CursorStyle::PointingHand)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        toggle_state.update(cx, |state, cx| state.toggle(window, cx));
                        cx.stop_propagation();
                    })
                    .children(self.trigger),
            )
            .children(menu)
    }
}

impl Styled for Dropdown {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
