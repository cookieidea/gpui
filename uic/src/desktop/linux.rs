use std::str::FromStr;

#[derive(Debug, strum::Display)]
#[allow(non_camel_case_types)]
#[allow(clippy::upper_case_acronyms)]
pub enum LinuxDesktop {
    GNOME,
    KDE,
    XFCE,
    LXQt,
    MATE,
    Cinnamon,
    sway,
    niri,
    Hyprland,
    Unknown,
}

impl FromStr for LinuxDesktop {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let val = match s {
            "GNOME" => Self::GNOME,
            "KDE" => Self::KDE,
            "XFCE" => Self::XFCE,
            "LXQt" => Self::LXQt,
            "MATE" => Self::MATE,
            "Cinnamon" => Self::MATE,
            "sway" => Self::sway,
            "niri" => Self::niri,
            "Hyprland" => Self::Hyprland,
            _ => Self::Unknown,
        };
        Ok(val)
    }
}
