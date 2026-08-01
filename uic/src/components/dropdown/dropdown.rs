use gpui::{
    AnyElement, App, CursorStyle, Entity, Focusable, IntoElement, MouseButton, RenderOnce, Window,
    deferred, div, prelude::*,
};

use super::{DropdownAppearance, DropdownPlacement, DropdownState};

#[derive(IntoElement)]
pub struct Dropdown {
    state: Entity<DropdownState>,
    trigger: Option<AnyElement>,
    menu: Option<AnyElement>,
    appearance: DropdownAppearance,
    placement: DropdownPlacement,
    priority: usize,
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
            appearance: DropdownAppearance::default(),
            placement: DropdownPlacement::default(),
            priority: 100,
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

    pub fn appearance(mut self, appearance: DropdownAppearance) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn placement(mut self, placement: DropdownPlacement) -> Self {
        self.placement = placement;
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
        let appearance = self.appearance;

        let menu = open.then(|| {
            let positioned = match self.placement {
                DropdownPlacement::BottomStart => div().absolute().top_full().left_0(),
                DropdownPlacement::BottomEnd => div().absolute().top_full().right_0(),
                DropdownPlacement::TopStart => div().absolute().bottom_full().left_0(),
                DropdownPlacement::TopEnd => div().absolute().bottom_full().right_0(),
            };

            deferred(
                positioned
                    .id(("dropdown-menu", state_id))
                    .mt(match self.placement {
                        DropdownPlacement::BottomStart | DropdownPlacement::BottomEnd => {
                            appearance.gap
                        }
                        DropdownPlacement::TopStart | DropdownPlacement::TopEnd => gpui::px(0.),
                    })
                    .mb(match self.placement {
                        DropdownPlacement::TopStart | DropdownPlacement::TopEnd => appearance.gap,
                        DropdownPlacement::BottomStart | DropdownPlacement::BottomEnd => {
                            gpui::px(0.)
                        }
                    })
                    .min_w(appearance.min_width)
                    .max_h(appearance.max_height)
                    .overflow_y_scroll()
                    .p(appearance.padding)
                    .rounded(appearance.radius)
                    .border(appearance.border_width)
                    .border_color(appearance.border)
                    .bg(appearance.background)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(self.menu),
            )
            .with_priority(self.priority)
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
