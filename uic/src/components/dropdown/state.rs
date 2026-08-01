use gpui::{App, Context, FocusHandle, Focusable, Subscription, Window};

pub struct DropdownState {
    open: bool,
    focus_handle: FocusHandle,
    previous_focus: Option<FocusHandle>,
    _subscriptions: Vec<Subscription>,
}

impl DropdownState {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let blur_subscription = cx.on_blur(&focus_handle, window, |state, window, cx| {
            if state.open {
                state.open = false;
                state.previous_focus = None;
                window.refresh();
                cx.notify();
            }
        });

        Self {
            open: false,
            focus_handle,
            previous_focus: None,
            _subscriptions: vec![blur_subscription],
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.previous_focus = window.focused(cx);
        }
        self.open = true;
        self.focus_handle.focus(window, cx);
        window.refresh();
        cx.notify();
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            if let Some(previous_focus) = self.previous_focus.take() {
                previous_focus.focus(window, cx);
            }
            window.refresh();
            cx.notify();
        }
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.close(window, cx);
        } else {
            self.open(window, cx);
        }
    }
}

impl Focusable for DropdownState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
