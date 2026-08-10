use anyhow::{Context as _, Result, anyhow};
use gpui::{Action, Menu, MenuItem, TrayIcon, TrayId, TrayOptions};
use std::{ffi::c_void, mem, ptr};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, WPARAM},
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
            DeleteObject, HGDIOBJ,
        },
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
                NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, HICON,
                HMENU, ICONINFO, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
                PostMessageW, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
                WM_NULL,
            },
        },
    },
    core::PCWSTR,
};

pub(crate) const WM_GPUI_TRAY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_USER + 9;

pub(crate) struct WindowsTray {
    notify_data: NOTIFYICONDATAW,
    icon: HICON,
    menu: HMENU,
    actions: Vec<Box<dyn Action>>,
    activate: Option<Box<dyn Action>>,
}

impl WindowsTray {
    pub(crate) fn new(id: TrayId, hwnd: HWND, options: TrayOptions) -> Result<Self> {
        anyhow::ensure!(
            id.as_u32() <= u16::MAX as u32,
            "Windows tray identifiers must fit in 16 bits"
        );
        let icon = create_icon(&options.icon)?;
        let mut actions = Vec::new();
        let menu = match create_menu(&options.menu, &mut actions) {
            Ok(menu) => menu,
            Err(error) => {
                unsafe { DestroyIcon(icon).ok() };
                return Err(error);
            }
        };

        let mut notify_data = NOTIFYICONDATAW {
            cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: id.as_u32(),
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_SHOWTIP | NIF_TIP,
            uCallbackMessage: WM_GPUI_TRAY,
            hIcon: icon,
            ..Default::default()
        };
        if let Some(tooltip) = options.tooltip {
            write_wide_buffer(&mut notify_data.szTip, &tooltip);
        }

        Ok(Self {
            notify_data,
            icon,
            menu,
            actions,
            activate: options.activate,
        })
    }

    pub(crate) fn add(&mut self) -> Result<()> {
        anyhow::ensure!(
            unsafe { Shell_NotifyIconW(NIM_ADD, &self.notify_data) }.as_bool(),
            "Shell_NotifyIconW(NIM_ADD) failed"
        );
        self.notify_data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if !unsafe { Shell_NotifyIconW(NIM_SETVERSION, &self.notify_data) }.as_bool() {
            self.delete();
            anyhow::bail!("Shell_NotifyIconW(NIM_SETVERSION) failed");
        }
        Ok(())
    }

    pub(crate) fn modify(&self) -> Result<()> {
        anyhow::ensure!(
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &self.notify_data) }.as_bool(),
            "Shell_NotifyIconW(NIM_MODIFY) failed"
        );
        Ok(())
    }

    pub(crate) fn delete(&self) {
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &self.notify_data) };
    }

    pub(crate) fn activation_action(&self) -> Option<Box<dyn Action>> {
        self.activate.as_ref().map(|action| action.boxed_clone())
    }

    pub(crate) fn show_menu(&self, hwnd: HWND, position: POINT) -> Option<Box<dyn Action>> {
        if self.menu.is_invalid() {
            return None;
        }
        let _ = unsafe { SetForegroundWindow(hwnd) };
        let command = unsafe {
            TrackPopupMenu(
                self.menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                position.x,
                position.y,
                None,
                hwnd,
                None,
            )
        };
        unsafe { PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0)).ok() };
        let index = command.0 as usize;
        (index > 0)
            .then(|| self.actions.get(index - 1))
            .flatten()
            .map(|action| action.boxed_clone())
    }
}

impl Drop for WindowsTray {
    fn drop(&mut self) {
        unsafe {
            if !self.menu.is_invalid() {
                DestroyMenu(self.menu).ok();
            }
            if !self.icon.is_invalid() {
                DestroyIcon(self.icon).ok();
            }
        }
    }
}

fn create_menu(items: &[MenuItem], actions: &mut Vec<Box<dyn Action>>) -> Result<HMENU> {
    let menu = unsafe { CreatePopupMenu() }.context("creating tray popup menu")?;
    let result = (|| {
        for item in items {
            let result = match item {
                MenuItem::Separator => unsafe {
                    AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null())
                },
                MenuItem::Label(label) => {
                    let label = wide(label);
                    unsafe { AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, PCWSTR(label.as_ptr())) }
                }
                MenuItem::Action {
                    name,
                    action,
                    checked,
                    disabled,
                    ..
                } => {
                    actions.push(action.boxed_clone());
                    let command = actions.len();
                    let mut flags = MF_STRING;
                    if *checked {
                        flags |= MF_CHECKED;
                    }
                    if *disabled {
                        flags |= MF_GRAYED;
                    }
                    let name = wide(name);
                    unsafe { AppendMenuW(menu, flags, command, PCWSTR(name.as_ptr())) }
                }
                MenuItem::Submenu(Menu {
                    name,
                    items,
                    disabled,
                }) => {
                    let submenu = create_menu(items, actions)?;
                    let mut flags = MF_POPUP | MF_STRING;
                    if *disabled {
                        flags |= MF_GRAYED;
                    }
                    let name = wide(name);
                    let result = unsafe {
                        AppendMenuW(menu, flags, submenu.0 as usize, PCWSTR(name.as_ptr()))
                    };
                    if result.is_err() {
                        unsafe { DestroyMenu(submenu).ok() };
                    }
                    result
                }
                MenuItem::SystemMenu(_) => continue,
            };
            result.map_err(|error| anyhow!(error))?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        unsafe { DestroyMenu(menu).ok() };
        return Err(error);
    }
    Ok(menu)
}

fn create_icon(icon: &TrayIcon) -> Result<HICON> {
    let image = icon
        .images()
        .iter()
        .max_by_key(|image| image.width.saturating_mul(image.height))
        .ok_or_else(|| anyhow!("tray icon has no image representations"))?;
    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: image.width as i32,
            biHeight: -(image.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = ptr::null_mut::<c_void>();
    let color = unsafe { CreateDIBSection(None, &bitmap_info, DIB_RGB_COLORS, &mut bits, None, 0) }
        .context("creating tray icon color bitmap")?;
    if bits.is_null() {
        let _ = unsafe { DeleteObject(HGDIOBJ(color.0)) };
        return Err(anyhow!("CreateDIBSection returned a null pixel buffer"));
    }

    let target = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), image.rgba.len()) };
    for (source, target) in image.rgba.chunks_exact(4).zip(target.chunks_exact_mut(4)) {
        let alpha = source[3] as u16;
        let premultiply = |channel: u8| ((channel as u16 * alpha + 127) / 255) as u8;
        target.copy_from_slice(&[
            premultiply(source[2]),
            premultiply(source[1]),
            premultiply(source[0]),
            source[3],
        ]);
    }

    let mask = unsafe { CreateBitmap(image.width as i32, image.height as i32, 1, 1, None) };
    if mask.is_invalid() {
        let _ = unsafe { DeleteObject(HGDIOBJ(color.0)) };
        return Err(anyhow!("creating tray icon mask bitmap failed"));
    }
    let icon = unsafe {
        CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            hbmMask: mask,
            hbmColor: color,
            ..Default::default()
        })
    };
    unsafe {
        let _ = DeleteObject(HGDIOBJ(mask.0));
        let _ = DeleteObject(HGDIOBJ(color.0));
    }
    icon.context("creating tray icon")
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn write_wide_buffer<const N: usize>(target: &mut [u16; N], value: &str) {
    let encoded = value.encode_utf16().take(N.saturating_sub(1));
    for (target, value) in target.iter_mut().zip(encoded) {
        *target = value;
    }
}
