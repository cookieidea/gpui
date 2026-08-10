use calloop::channel::Sender;
use gpui::{Action, Menu, MenuItem, TrayEvent, TrayId, TrayOptions, TrayScrollAxis};
use ksni::menu;

pub(crate) enum LinuxTrayMessage {
    Event(TrayId, TrayEvent),
    Action(Box<dyn Action>),
}

pub(crate) struct GpuiTray {
    id: TrayId,
    service_id: String,
    options: TrayOptions,
    sender: Sender<LinuxTrayMessage>,
}

impl GpuiTray {
    pub(crate) fn new(id: TrayId, options: TrayOptions, sender: Sender<LinuxTrayMessage>) -> Self {
        let executable = std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "gpui".to_string());
        Self {
            id,
            service_id: format!("{executable}-{}", id.as_u32()),
            options,
            sender,
        }
    }

    pub(crate) fn replace_options(&mut self, options: TrayOptions) {
        self.options = options;
    }

    pub(crate) fn spawn(self) -> anyhow::Result<ksni::Handle<Self>> {
        smol::block_on(<Self as ksni::TrayMethods>::spawn(self))
            .map_err(|error| anyhow::anyhow!(error))
    }
}

impl ksni::Tray for GpuiTray {
    fn id(&self) -> String {
        self.service_id.clone()
    }

    fn title(&self) -> String {
        self.options
            .tooltip
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| self.service_id.clone())
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.options
            .icon
            .images()
            .iter()
            .map(|image| {
                let mut argb = image.rgba.to_vec();
                for pixel in argb.chunks_exact_mut(4) {
                    pixel.rotate_right(1);
                }
                ksni::Icon {
                    width: image.width as i32,
                    height: image.height as i32,
                    data: argb,
                }
            })
            .collect()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self
                .options
                .tooltip
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.sender
            .send(LinuxTrayMessage::Event(self.id, TrayEvent::PrimaryActivate))
            .ok();
        if let Some(action) = &self.options.activate {
            self.sender
                .send(LinuxTrayMessage::Action(action.boxed_clone()))
                .ok();
        }
    }

    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        self.sender
            .send(LinuxTrayMessage::Event(
                self.id,
                TrayEvent::SecondaryActivate,
            ))
            .ok();
    }

    fn scroll(&mut self, delta: i32, orientation: ksni::Orientation) {
        let axis = match orientation {
            ksni::Orientation::Horizontal => TrayScrollAxis::Horizontal,
            ksni::Orientation::Vertical => TrayScrollAxis::Vertical,
        };
        self.sender
            .send(LinuxTrayMessage::Event(
                self.id,
                TrayEvent::Scroll {
                    delta: delta as f32,
                    axis,
                },
            ))
            .ok();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        convert_menu(&self.options.menu, &self.sender)
    }
}

fn convert_menu(
    items: &[MenuItem],
    sender: &Sender<LinuxTrayMessage>,
) -> Vec<ksni::MenuItem<GpuiTray>> {
    items
        .iter()
        .filter_map(|item| match item {
            MenuItem::Separator => Some(ksni::MenuItem::Separator),
            MenuItem::Label(label) => Some(
                menu::StandardItem {
                    label: escape_label(label),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            ),
            MenuItem::Action {
                name,
                action,
                checked,
                disabled,
                ..
            } => {
                let sender = sender.clone();
                let action = action.boxed_clone();
                if *checked {
                    Some(
                        menu::CheckmarkItem {
                            label: escape_label(name),
                            enabled: !disabled,
                            checked: true,
                            activate: Box::new(move |_| {
                                sender
                                    .send(LinuxTrayMessage::Action(action.boxed_clone()))
                                    .ok();
                            }),
                            ..Default::default()
                        }
                        .into(),
                    )
                } else {
                    Some(
                        menu::StandardItem {
                            label: escape_label(name),
                            enabled: !disabled,
                            activate: Box::new(move |_| {
                                sender
                                    .send(LinuxTrayMessage::Action(action.boxed_clone()))
                                    .ok();
                            }),
                            ..Default::default()
                        }
                        .into(),
                    )
                }
            }
            MenuItem::Submenu(Menu {
                name,
                items,
                disabled,
            }) => Some(
                menu::SubMenu {
                    label: escape_label(name),
                    enabled: !disabled,
                    submenu: convert_menu(items, sender),
                    ..Default::default()
                }
                .into(),
            ),
            MenuItem::SystemMenu(_) => None,
        })
        .collect()
}

fn escape_label(label: &str) -> String {
    label.replace('_', "__")
}
