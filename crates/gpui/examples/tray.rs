use gpui::{
    App, Global, Menu, MenuItem, QuitMode, TrayEvent, TrayIcon, TrayId, TrayOptions, actions,
};
use gpui_platform::application;

actions!(tray_example, [Activate, ToggleCheck, Quit]);

struct TrayState {
    id: TrayId,
    icon: TrayIcon,
    checked: bool,
}

impl Global for TrayState {}

fn tray_options(icon: TrayIcon, checked: bool) -> TrayOptions {
    TrayOptions::new(icon)
        .tooltip("GPUI tray example")
        .on_activate(Activate)
        .menu([
            MenuItem::label("GPUI Tray Example"),
            MenuItem::separator(),
            MenuItem::action("Primary action", Activate),
            MenuItem::action("Checked item", ToggleCheck).checked(checked),
            MenuItem::action("Disabled item", gpui::NoAction).disabled(true),
            MenuItem::submenu(Menu::new("More").items([
                MenuItem::label("Native submenu"),
                MenuItem::action("Toggle check", ToggleCheck),
            ])),
            MenuItem::separator(),
            MenuItem::action("Quit", Quit),
        ])
}

fn make_icon(size: u32) -> TrayIcon {
    let mut rgba = vec![0_u8; (size * size * 4) as usize];
    let center = (size as f32 - 1.0) / 2.0;
    let radius = size as f32 * 0.44;

    for y in 0..size {
        for x in 0..size {
            let offset = ((y * size + x) * 4) as usize;
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            if dx * dx + dy * dy <= radius * radius {
                rgba[offset..offset + 4].copy_from_slice(&[0x22, 0x88, 0xee, 0xff]);
                if dx.abs() < size as f32 * 0.08 || dy.abs() < size as f32 * 0.08 {
                    rgba[offset..offset + 4].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
                }
            }
        }
    }

    TrayIcon::from_rgba(rgba, size, size).expect("generated icon has valid dimensions")
}

fn main() {
    application().run(|cx: &mut App| {
        cx.set_quit_mode(QuitMode::Explicit);
        cx.on_action(|_: &Activate, _| println!("tray activated"));
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &ToggleCheck, cx| {
            let (id, icon, checked) = {
                let state = cx.global_mut::<TrayState>();
                state.checked = !state.checked;
                (state.id, state.icon.clone(), state.checked)
            };
            cx.update_tray(id, tray_options(icon, checked)).unwrap();
        });

        let icon = make_icon(32);
        let id = cx
            .create_tray(tray_options(icon.clone(), false))
            .expect("this platform does not support system tray items");
        cx.set_global(TrayState {
            id,
            icon,
            checked: false,
        });
        cx.on_tray_event(id, |_, event| match event {
            TrayEvent::PrimaryActivate => println!("primary tray activation"),
            TrayEvent::SecondaryActivate => println!("secondary tray activation"),
            TrayEvent::Scroll { delta, axis } => {
                println!("tray scrolled {delta} on {axis:?}")
            }
        })
        .detach();
    });
}
