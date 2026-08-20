use std::{cell::Cell, rc::Rc};

use gpui::{
    App, Bounds, DispatchPhase, Element, ElementId, FocusHandle, GlobalElementId, Hitbox,
    HitboxBehavior, HitboxId, InspectorElementId, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Refineable as _, Style, StyleRefinement, Styled, Window,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InteractionPhase {
    Hover,
    Start,
    Preview,
    Commit,
}

type Callback = Rc<dyn Fn(f32, InteractionPhase, &mut Window, &mut App)>;

#[derive(Clone, Default)]
pub(super) struct CaptureToken(Rc<Cell<Option<HitboxId>>>);

pub(super) struct SliderInteraction {
    capture_token: CaptureToken,
    focus_handle: FocusHandle,
    callback: Callback,
    edge_inset: Pixels,
    style: StyleRefinement,
}

impl SliderInteraction {
    pub(super) fn new(
        capture_token: CaptureToken,
        focus_handle: FocusHandle,
        callback: impl Fn(f32, InteractionPhase, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            capture_token,
            focus_handle,
            callback: Rc::new(callback),
            edge_inset: Pixels::ZERO,
            style: StyleRefinement::default(),
        }
    }

    pub(super) fn edge_inset(mut self, edge_inset: Pixels) -> Self {
        self.edge_inset = edge_inset.max(Pixels::ZERO);
        self
    }
}

impl IntoElement for SliderInteraction {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SliderInteraction {
    type RequestLayoutState = Style;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.refine(&self.style);
        let layout_id = window.request_layout(style.clone(), [], cx);
        (layout_id, style)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Style,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        if let Some(previous_hitbox) = self.capture_token.0.get()
            && window.captured_hitbox() == Some(previous_hitbox)
        {
            window.capture_pointer(hitbox.id);
        }
        self.capture_token.0.set(Some(hitbox.id));
        hitbox
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        style: &mut Style,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let down_hitbox = hitbox.clone();
        let down_focus = self.focus_handle.clone();
        let down_callback = self.callback.clone();
        let down_inset = self.edge_inset;
        let move_hitbox = hitbox.clone();
        let move_callback = self.callback.clone();
        let move_inset = self.edge_inset;
        let up_hitbox = hitbox.clone();
        let up_callback = self.callback.clone();
        let up_inset = self.edge_inset;

        style.paint(bounds, window, cx, move |window, _| {
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && event.button == MouseButton::Left
                    && down_hitbox.is_hovered(window)
                {
                    window.capture_pointer(down_hitbox.id);
                    down_focus.focus(window, cx);
                    down_callback(
                        horizontal_ratio(event.position.x, down_hitbox.bounds, down_inset),
                        InteractionPhase::Start,
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            });
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture
                    && event.dragging()
                    && window.captured_hitbox() == Some(move_hitbox.id)
                {
                    move_callback(
                        horizontal_ratio(event.position.x, move_hitbox.bounds, move_inset),
                        InteractionPhase::Preview,
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                } else if phase == DispatchPhase::Bubble
                    && !event.dragging()
                    && move_hitbox.is_hovered(window)
                {
                    move_callback(
                        horizontal_ratio(event.position.x, move_hitbox.bounds, move_inset),
                        InteractionPhase::Hover,
                        window,
                        cx,
                    );
                }
            });
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture
                    && event.button == MouseButton::Left
                    && window.captured_hitbox() == Some(up_hitbox.id)
                {
                    up_callback(
                        horizontal_ratio(event.position.x, up_hitbox.bounds, up_inset),
                        InteractionPhase::Commit,
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            });
        });
    }
}

impl Styled for SliderInteraction {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn horizontal_ratio(x: Pixels, bounds: Bounds<Pixels>, inset: Pixels) -> f32 {
    let width = f32::from(bounds.size.width - inset * 2.).max(1.0);
    (f32::from(x - bounds.origin.x - inset) / width).clamp(0.0, 1.0)
}
