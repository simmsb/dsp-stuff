pub struct Theme {
    pub titlebar: egui::Color32,
    pub titlebar_hovered: egui::Color32,

    pub text: egui::Color32,
    pub dark: bool,

    pub grid_background: egui::Color32,

    pub node_background: egui::Color32,
    pub node_background_hovered: egui::Color32,

    pub link: egui::Color32,
    pub link_hovered: egui::Color32,
}

pub static MONOKAI: Theme = Theme {
    dark: true,
    titlebar: egui::Color32::from_rgba_premultiplied(0x5b, 0x53, 0x53, 0xff),
    titlebar_hovered: egui::Color32::from_rgba_premultiplied(0x72, 0x69, 0x6a, 0xff),
    text: egui::Color32::from_rgba_premultiplied(0xfd, 0xf8, 0xf9, 0xff),
    grid_background: egui::Color32::from_rgba_premultiplied(0x2c, 0x25, 0x25, 0xff),
    node_background: egui::Color32::from_rgba_premultiplied(0x40, 0x38, 0x38, 0xff),
    node_background_hovered: egui::Color32::from_rgba_premultiplied(0x5b, 0x53, 0x53, 0xff),
    link: egui::Color32::from_rgba_premultiplied(0xa8, 0xa9, 0xeb, 0xff),
    link_hovered: egui::Color32::from_rgba_premultiplied(0xb8, 0xb9, 0xfb, 0xff),
};

pub static SOLARIZED: Theme = Theme {
    dark: true,
    titlebar: egui::Color32::from_rgba_premultiplied(0x58, 0x6e, 0x75, 0xff),
    titlebar_hovered: egui::Color32::from_rgba_premultiplied(0x65, 0x7b, 0x83, 0xff),
    text: egui::Color32::from_rgba_premultiplied(0xfd, 0xf6, 0xe3, 0xff),
    grid_background: egui::Color32::from_rgba_premultiplied(0x00, 0x2b, 0x36, 0xff),
    node_background: egui::Color32::from_rgba_premultiplied(0x07, 0x36, 0x42, 0xff),
    node_background_hovered: egui::Color32::from_rgba_premultiplied(0x58, 0x6e, 0x75, 0xff),
    link: egui::Color32::from_rgba_premultiplied(0x6c, 0x71, 0xc4, 0xff),
    link_hovered: egui::Color32::from_rgba_premultiplied(0x26, 0x8b, 0xd2, 0xff),
};

pub static THEMES: &[(&str, &Theme)] = &[("Monokai", &MONOKAI), ("Solarized", &SOLARIZED)];
