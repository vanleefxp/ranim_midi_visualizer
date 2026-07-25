use crate::action::*;
use crate::state::*;
use gpui::*;
use gpui_component::GlobalState;
use tracing::info;

fn app_menus(cx: &App) -> Vec<OwnedMenu> {
    vec![
        Menu::new("File")
            .items([
                MenuItem::action("Open", ShowOpenDialog),
                MenuItem::submenu(Menu::new("Recent files").items({
                    let mut items = cx.read_entity(&file::recent_files(cx), |v, _cx| {
                        info!("Recent files: {:?}", v);
                        if v.is_empty() {
                            vec![MenuItem::action("(No recent files)", NoAction).disabled(true)]
                        } else {
                            v.iter()
                                .map(|v| {
                                    MenuItem::action(v.display().to_string(), OpenFile(v.clone()))
                                })
                                .collect::<Vec<_>>()
                        }
                    });
                    items.extend([
                        MenuItem::separator(),
                        MenuItem::action("Clear", ClearRecentFiles),
                    ]);
                    items
                })),
                MenuItem::separator(),
                MenuItem::action("Export", ExportVideo),
            ])
            .owned(),
        Menu::new("Style")
            .items([
                MenuItem::action("Save Style", SaveStyle),
                MenuItem::action("Load Style", LoadStyle),
                MenuItem::separator(),
                MenuItem::action("Revert to Default", RevertToDefault),
            ])
            .owned(),
    ]
}

pub fn update_menus(cx: &mut App) {
    info!("Updating menus...");
    let app_menus = app_menus(cx);
    GlobalState::global_mut(cx).set_app_menus(app_menus);
    cx.update_global::<ShouldReloadMenuBar, _>(|g, _cx| **g = true);
}
