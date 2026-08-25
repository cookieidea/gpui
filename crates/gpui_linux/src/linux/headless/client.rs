use std::cell::RefCell;
use std::rc::Rc;

use calloop::{EventLoop, LoopHandle};
use gpui_util::ResultExt;

use crate::linux::headless::window::{HeadlessDisplay, HeadlessWindow};
use crate::linux::{LinuxClient, LinuxCommon, LinuxKeyboardLayout, dispatch_tray_message};
use gpui::{
    AnyWindowHandle, CursorStyle, DisplayId, PlatformDisplay, PlatformKeyboardLayout,
    PlatformWindow, WindowParams,
};

/// Creates virtual windows for applications driven by a non-compositor host.
pub trait HeadlessWindowFactory {
    /// Creates the platform window for a GPUI window handle.
    fn open_window(
        &self,
        handle: AnyWindowHandle,
        params: WindowParams,
        display: Rc<dyn PlatformDisplay>,
    ) -> anyhow::Result<Box<dyn PlatformWindow>>;
}

pub struct HeadlessClientState {
    pub(crate) _loop_handle: LoopHandle<'static, HeadlessClient>,
    pub(crate) event_loop: Option<calloop::EventLoop<'static, HeadlessClient>>,
    pub(crate) common: LinuxCommon,
    pub(crate) display: Rc<dyn PlatformDisplay>,
    pub(crate) window_factory: Option<Rc<dyn HeadlessWindowFactory>>,
}

#[derive(Clone)]
pub(crate) struct HeadlessClient(Rc<RefCell<HeadlessClientState>>);

impl HeadlessClient {
    pub(crate) fn new() -> Self {
        Self::with_window_factory(None)
    }

    pub(crate) fn with_window_factory(
        window_factory: Option<Rc<dyn HeadlessWindowFactory>>,
    ) -> Self {
        let event_loop = EventLoop::try_new().unwrap();

        let (common, main_receiver, wake_receiver, tray_receiver) =
            LinuxCommon::new(event_loop.get_signal());

        let handle = event_loop.handle();

        handle
            .insert_source(main_receiver, |event, _, _: &mut HeadlessClient| {
                if let calloop::channel::Event::Msg(runnable) = event {
                    runnable.run();
                }
            })
            .ok();

        handle
            .insert_source(wake_receiver, |event, _, client: &mut HeadlessClient| {
                if let calloop::channel::Event::Msg(()) = event {
                    client.with_common(|common| common.handle_system_wake());
                }
            })
            .ok();

        handle
            .insert_source(tray_receiver, |event, _, client: &mut HeadlessClient| {
                if let calloop::channel::Event::Msg(message) = event {
                    dispatch_tray_message(client, message);
                }
            })
            .ok();

        HeadlessClient(Rc::new(RefCell::new(HeadlessClientState {
            event_loop: Some(event_loop),
            _loop_handle: handle,
            common,
            display: Rc::new(HeadlessDisplay::new()),
            window_factory,
        })))
    }
}

impl LinuxClient for HeadlessClient {
    fn with_common<R>(&self, f: impl FnOnce(&mut LinuxCommon) -> R) -> R {
        f(&mut self.0.borrow_mut().common)
    }

    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(LinuxKeyboardLayout::new("unknown".into()))
    }

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        vec![self.0.borrow().display.clone()]
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(self.0.borrow().display.clone())
    }

    fn display(&self, id: DisplayId) -> Option<Rc<dyn PlatformDisplay>> {
        let display = self.0.borrow().display.clone();
        (display.id() == id).then_some(display)
    }

    #[cfg(feature = "screen-capture")]
    fn screen_capture_sources(
        &self,
    ) -> futures::channel::oneshot::Receiver<anyhow::Result<Vec<Rc<dyn gpui::ScreenCaptureSource>>>>
    {
        let (tx, rx) = futures::channel::oneshot::channel();
        tx.send(Err(anyhow::anyhow!(
            "Headless mode does not support screen capture."
        )))
        .ok();
        rx
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        None
    }

    fn window_stack(&self) -> Option<Vec<AnyWindowHandle>> {
        None
    }

    fn open_window(
        &self,
        handle: AnyWindowHandle,
        params: WindowParams,
    ) -> anyhow::Result<Box<dyn PlatformWindow>> {
        let state = self.0.borrow();
        if let Some(factory) = &state.window_factory {
            return factory.open_window(handle, params, state.display.clone());
        }
        Ok(Box::new(HeadlessWindow::new(params, state.display.clone())))
    }

    fn compositor_name(&self) -> &'static str {
        "headless"
    }

    fn set_cursor_style(&self, _style: CursorStyle) {}

    fn open_uri(&self, _uri: &str) {}

    fn reveal_path(&self, _path: std::path::PathBuf) {}

    fn write_to_primary(&self, _item: gpui::ClipboardItem) {}

    fn write_to_clipboard(&self, _item: gpui::ClipboardItem) {}

    fn read_from_primary(&self) -> Option<gpui::ClipboardItem> {
        None
    }

    fn read_from_clipboard(&self) -> Option<gpui::ClipboardItem> {
        None
    }

    fn run(&self) {
        let mut event_loop = self
            .0
            .borrow_mut()
            .event_loop
            .take()
            .expect("App is already running");

        event_loop.run(None, &mut self.clone(), |_| {}).log_err();
    }
}
