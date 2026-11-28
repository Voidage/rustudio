//! The CoreScripts browser: Roblox's built-in CoreScripts and CoreGui
//! modules, listed before Play. The developer can read them, copy one into
//! the place's `CoreScripts` service to edit it, and switch a CoreScript off
//! so it never starts (removing chat's GUI is not the same as stopping the
//! chat script). The engine reads the service at Play
//! (rustudio-engine/src/runtime.rs, start_core_script).

use std::collections::HashMap;

use egui::{Color32, RichText};
use rbx_dom_weak::types::{Ref, Variant};
use rbx_dom_weak::{ustr, InstanceBuilder};

use crate::app::Editor;
use rustudio_engine::theme;

#[derive(Default)]
pub struct CoreScriptsPanel {
    state: LoadState,
    filter: String,
    selected: Option<Selection>,
    preview: Option<Preview>,
    note: Option<(bool, String)>,
}

#[derive(Default)]
enum LoadState {
    #[default]
    Idle,
    Ready(rustudio_core::core_scripts::Catalog),
    Failed(String),
}

#[derive(Clone, PartialEq)]
enum Selection {
    Script(usize),
    Module(Vec<String>),
}

struct Preview {
    title: String,
    subtitle: String,
    /// The whole file, for Copy into this place.
    source: Option<String>,
    /// The first part of it, for the preview.
    shown: Option<String>,
    truncated: bool,
    /// A module the engine can replace (a ModuleScript, not a Folder or a
    /// script that only happens to carry Source).
    module_copy: bool,
}

#[derive(Clone, Copy)]
struct EntryInfo {
    r: Ref,
    disabled: bool,
    has_source: bool,
}

enum Action {
    Refresh,
    SelectScript(usize),
    SelectModule(Vec<String>),
    CopyScript(usize),
    CopyModule(Vec<String>, String),
    Revert(Ref),
    ToggleScript { name: String, enable: bool },
    Open(Ref),
}

impl CoreScriptsPanel {
    fn ensure_loaded(&mut self) {
        if matches!(self.state, LoadState::Idle) {
            self.state = match rustudio_core::core_scripts::catalog() {
                Ok(c) => LoadState::Ready(c),
                Err(e) => LoadState::Failed(e),
            };
        }
    }
}

impl Editor {
    pub fn core_scripts_ui(&mut self, ui: &mut egui::Ui) {
        let (entries, module_overrides) = core_script_place_state(&self.game.doc);
        let mut action = None;
        {
            let panel = &mut self.core_scripts;
            panel.ensure_loaded();
            // The top line is the same with or without the patch.
            ui.horizontal_wrapped(|ui| {
                ui.heading("CoreScripts");
                if ui.button("Refresh").clicked() {
                    action = Some(Action::Refresh);
                }
                ui.label(
                    RichText::new(
                        "Roblox's scripts, before Play. Copy one into this place to edit it; \
                         untick a CoreScript to stop it starting.",
                    )
                    .color(theme::TEXT_DIM),
                );
            });
            if let Some((error, note)) = &panel.note {
                ui.label(RichText::new(note).color(if *error { theme::ERROR } else { theme::TEXT }));
            }
            ui.separator();

            // The catalog is a field of `panel`; take it out while the list
            // draws so the same closure can change the selection and filter.
            let state = std::mem::replace(&mut panel.state, LoadState::Idle);
            egui::Panel::left("core_scripts_list")
                .resizable(true)
                .default_size(330.0)
                .min_size(220.0)
                .show_inside(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut panel.filter)
                            .hint_text("Filter scripts and modules…")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match &state {
                            LoadState::Idle => {
                                ui.label("Reading Roblox's scripts…");
                            }
                            LoadState::Failed(e) => {
                                ui.label(RichText::new(e).color(theme::ERROR));
                            }
                            LoadState::Ready(catalog) => {
                                draw_script_list(ui, panel, catalog, &entries, &mut action);
                                ui.add_space(8.0);
                                draw_module_list(ui, panel, catalog, &module_overrides, &mut action);
                            }
                        });
                });
            panel.state = state;

            egui::CentralPanel::default().show_inside(ui, |ui| {
                draw_preview(ui, panel, &entries, &module_overrides, &mut action);
            });
        }
        if let Some(action) = action {
            self.apply_core_scripts_action(action);
        }
    }

    fn apply_core_scripts_action(&mut self, action: Action) {
        match action {
            Action::Refresh => {
                self.core_scripts.state = LoadState::Idle;
                self.core_scripts.selected = None;
                self.core_scripts.preview = None;
                self.core_scripts.note = None;
                self.core_scripts.ensure_loaded();
            }
            Action::SelectScript(i) => {
                if let LoadState::Ready(catalog) = &self.core_scripts.state {
                    if let Some(s) = catalog.scripts.get(i) {
                        let source = s.source.clone();
                        let (shown, truncated) = source.as_deref().map(limit_preview).unzip();
                        self.core_scripts.preview = Some(Preview {
                            title: s.short.clone(),
                            subtitle: s.name.clone(),
                            source,
                            shown,
                            truncated: truncated.unwrap_or(false),
                            module_copy: false,
                        });
                        self.core_scripts.selected = Some(Selection::Script(i));
                    }
                }
            }
            Action::SelectModule(path) => {
                let source = rustudio_core::core_scripts::module_source(&path);
                let (shown, truncated) = source.as_deref().map(limit_preview).unzip();
                let title = path.last().cloned().unwrap_or_default();
                let subtitle = format!("CoreGui.RobloxGui.Modules.{}", path.join("."));
                let module_copy = match &self.core_scripts.state {
                    LoadState::Ready(catalog) => module_node(&catalog.modules, &path)
                        .is_some_and(|n| n.class == "ModuleScript"),
                    _ => false,
                };
                self.core_scripts.preview = Some(Preview {
                    title,
                    subtitle,
                    source,
                    shown,
                    truncated: truncated.unwrap_or(false),
                    module_copy,
                });
                self.core_scripts.selected = Some(Selection::Module(path));
            }
            Action::CopyScript(i) => {
                let (name, source) = match &self.core_scripts.state {
                    LoadState::Ready(catalog) => match catalog.scripts.get(i) {
                        Some(s) => (s.name.clone(), s.source.clone()),
                        None => return,
                    },
                    _ => return,
                };
                let Some(source) = source else {
                    self.core_scripts.note = Some((true, "This install has no Lua source file for that script.".into()));
                    return;
                };
                match self.copy_core_script(&name, source) {
                    Ok(r) => {
                        self.core_scripts.note = Some((false, format!("Copied {name} into CoreScripts.")));
                        self.open_script.push(r);
                    }
                    Err(e) => self.core_scripts.note = Some((true, e)),
                }
            }
            Action::CopyModule(path, source) => match self.copy_core_script_module(&path, source) {
                Ok(r) => {
                    self.core_scripts.note =
                        Some((false, format!("Copied {} into CoreScripts.Modules.", path.join("."))));
                    self.open_script.push(r);
                }
                Err(e) => self.core_scripts.note = Some((true, e)),
            },
            Action::Revert(r) => {
                if self.game.doc.remove(&[r]) > 0 {
                    self.core_scripts.note = Some((false, "Removed the place's copy; the built-in runs again.".into()));
                }
            }
            Action::ToggleScript { name, enable } => match self.set_core_script_enabled(&name, enable) {
                Ok(()) => {
                    self.core_scripts.note = Some((
                        false,
                        if enable {
                            format!("{name} will start at Play.")
                        } else {
                            format!("{name} will not start at Play.")
                        },
                    ));
                }
                Err(e) => self.core_scripts.note = Some((true, e)),
            },
            Action::Open(r) => self.open_script.push(r),
        }
    }

    fn core_scripts_service(&mut self) -> Ref {
        let root = self.game.doc.dom.root_ref();
        if let Some(r) = self.game.doc.find_child(root, "CoreScripts") {
            if self.game.doc.class_of(r).is_some_and(|c| c == "CoreScripts") {
                return r;
            }
        }
        self.game.doc.insert(
            root,
            InstanceBuilder::new("CoreScripts").with_name("CoreScripts"),
        )
    }

    /// Puts `source` in the service as the script Roblox starts by `name`,
    /// replacing any earlier copy (and enabling it).
    fn copy_core_script(&mut self, name: &str, source: String) -> Result<Ref, String> {
        let svc = self.core_scripts_service();
        if let Some(r) = find_service_script(&self.game.doc, svc, name) {
            if self.game.doc.class_of(r).is_some_and(|c| c != "CoreScript") {
                return Err(format!("{name} in CoreScripts is not a CoreScript."));
            }
            self.game.doc.set_props(
                "Override CoreScript",
                vec![
                    (r, ustr("Source"), Variant::String(source)),
                    (r, ustr("Disabled"), Variant::Bool(false)),
                ],
                None,
            );
            return Ok(r);
        }
        Ok(self.game.doc.insert(
            svc,
            InstanceBuilder::new("CoreScript")
                .with_name(name)
                .with_property("Source", source)
                .with_property("Disabled", false),
        ))
    }

    /// Copies `source` into `CoreScripts.Modules.<path>`, creating the
    /// folders that mirror the built-in module tree.
    fn copy_core_script_module(&mut self, path: &[String], source: String) -> Result<Ref, String> {
        let Some(leaf) = path.last().cloned() else {
            return Err("No module selected.".into());
        };
        let svc = self.core_scripts_service();
        let mut parent = match named_child(&self.game.doc, svc, "Modules") {
            Some(r) => r,
            None => self
                .game.doc
                .insert(svc, InstanceBuilder::new("Folder").with_name("Modules")),
        };
        for seg in &path[..path.len() - 1] {
            parent = match named_child(&self.game.doc, parent, seg) {
                Some(r) => {
                    if self.game.doc.class_of(r).is_some_and(|c| c != "Folder") {
                        return Err(format!("CoreScripts.Modules.{} is not a Folder.", seg));
                    }
                    r
                }
                None => self
                    .game.doc
                    .insert(parent, InstanceBuilder::new("Folder").with_name(seg.as_str())),
            };
        }
        match named_child(&self.game.doc, parent, &leaf) {
            Some(r) => {
                if self.game.doc.class_of(r).is_some_and(|c| c != "ModuleScript") {
                    return Err(format!("CoreScripts.Modules has a non-module {leaf}."));
                }
                self.game.doc.set_props(
                    "Override module",
                    vec![(r, ustr("Source"), Variant::String(source))],
                    None,
                );
                Ok(r)
            }
            None => Ok(self.game.doc.insert(
                parent,
                InstanceBuilder::new("ModuleScript")
                    .with_name(leaf)
                    .with_property("Source", source),
            )),
        }
    }

    /// The browser's Enabled switch: an entry with Source is an override that
    /// is on or off; a source-less entry only records "off" (there is nothing
    /// of the developer's to run), and removing it restores the built-in.
    fn set_core_script_enabled(&mut self, name: &str, enable: bool) -> Result<(), String> {
        let svc = self.core_scripts_service();
        let entry = find_service_script(&self.game.doc, svc, name);
        if enable {
            if let Some(r) = entry {
                let has_source = matches!(
                    self.game.doc.prop(r, "Source"),
                    Some(Variant::String(s)) if !s.is_empty()
                );
                if has_source {
                    self.game.doc.set_props(
                        "Enable CoreScript",
                        vec![(r, ustr("Disabled"), Variant::Bool(false))],
                        None,
                    );
                } else {
                    self.game.doc.remove(&[r]);
                }
            }
        } else if let Some(r) = entry {
            self.game.doc.set_props(
                "Disable CoreScript",
                vec![(r, ustr("Disabled"), Variant::Bool(true))],
                None,
            );
        } else {
            self.game.doc.insert(
                svc,
                InstanceBuilder::new("CoreScript")
                    .with_name(name)
                    .with_property("Disabled", true),
            );
        }
        // Play already running: flip the live copy too, so unticking chat
        // stops the chat script now (and ticking it starts from its Source),
        // exactly as setting Disabled on the live instance does.
        self.set_live_core_scripts(name, enable);
        Ok(())
    }

    fn set_live_core_scripts(&mut self, name: &str, enable: bool) {
        if self.game.play_status().is_none() {
            return;
        }
        let escaped = name.replace('\\', "\\\\").replace('\'', "\\'");
        let want = if enable { "false" } else { "true" };
        let code = format!(
            "for _, d in ipairs(game.CoreGui:GetDescendants()) do\n\
             \tif d:IsA('CoreScript') and d.Name == '{escaped}' then\n\
             \t\td.Disabled = {want}\n\
             \tend\n\
             end\n"
        );
        let _ = self.game.run_luau(rustudio_engine::runtime::Side::Client, &code);
    }
}

fn limit_preview(source: &str) -> (String, bool) {
    const MAX: usize = 24_000;
    if source.len() <= MAX {
        return (source.to_string(), false);
    }
    let mut cut = MAX;
    while !source.is_char_boundary(cut) {
        cut -= 1;
    }
    (source[..cut].to_string(), true)
}

fn module_node<'a>(
    nodes: &'a [rustudio_core::core_scripts::CatalogModule],
    path: &[String],
) -> Option<&'a rustudio_core::core_scripts::CatalogModule> {
    let (first, rest) = path.split_first()?;
    let node = nodes.iter().find(|n| n.name == *first)?;
    if rest.is_empty() {
        Some(node)
    } else {
        module_node(&node.children, rest)
    }
}

fn find_service_script(d: &rustudio_core::doc::Document, svc: Ref, name: &str) -> Option<Ref> {
    let node = d.dom.get_by_ref(svc)?;
    node.children().iter().copied().find(|r| {
        d.dom
            .get_by_ref(*r)
            .is_some_and(|i| i.class == "CoreScript" && i.name == name)
    })
}

/// A direct child by name (Document::find_child looks a class up).
fn named_child(d: &rustudio_core::doc::Document, parent: Ref, name: &str) -> Option<Ref> {
    let node = d.dom.get_by_ref(parent)?;
    node.children().iter().copied().find(|c| {
        d.dom
            .get_by_ref(*c)
            .is_some_and(|i| i.name == name)
    })
}

fn service_ref(d: &rustudio_core::doc::Document) -> Option<Ref> {
    let root = d.dom.root_ref();
    d.find_child(root, "CoreScripts")
        .filter(|r| d.class_of(*r).is_some_and(|c| c == "CoreScripts"))
}

/// The place's entries, for the list's badges and switches: direct
/// CoreScripts by Roblox's name, and the module copies by their path.
fn core_script_place_state(
    d: &rustudio_core::doc::Document,
) -> (HashMap<String, EntryInfo>, HashMap<Vec<String>, Ref>) {
    let mut entries = HashMap::new();
    let mut modules = HashMap::new();
    let Some(svc) = service_ref(d) else { return (entries, modules) };
    let Some(node) = d.dom.get_by_ref(svc) else { return (entries, modules) };
    for &c in node.children() {
        let Some(i) = d.dom.get_by_ref(c) else { continue };
        if i.class == "CoreScript" {
            entries.insert(
                i.name.clone(),
                EntryInfo {
                    r: c,
                    disabled: matches!(d.prop(c, "Disabled"), Some(Variant::Bool(true)))
                        || matches!(d.prop(c, "Enabled"), Some(Variant::Bool(false))),
                    has_source: matches!(d.prop(c, "Source"), Some(Variant::String(s)) if !s.is_empty()),
                },
            );
        }
    }
    if let Some(root) = named_child(d, svc, "Modules") {
        let mut path = Vec::new();
        collect_module_overrides(d, root, &mut path, &mut modules);
    }
    (entries, modules)
}

fn collect_module_overrides(
    d: &rustudio_core::doc::Document,
    parent: Ref,
    path: &mut Vec<String>,
    out: &mut HashMap<Vec<String>, Ref>,
) {
    let Some(node) = d.dom.get_by_ref(parent) else { return };
    for &c in node.children() {
        let Some(i) = d.dom.get_by_ref(c) else { continue };
        path.push(i.name.clone());
        if i.class == "ModuleScript"
            && matches!(d.prop(c, "Source"), Some(Variant::String(s)) if !s.is_empty())
        {
            out.insert(path.clone(), c);
        }
        collect_module_overrides(d, c, path, out);
        path.pop();
    }
}

fn draw_script_list(
    ui: &mut egui::Ui,
    panel: &mut CoreScriptsPanel,
    catalog: &rustudio_core::core_scripts::Catalog,
    entries: &HashMap<String, EntryInfo>,
    action: &mut Option<Action>,
) {
    let filter = panel.filter.trim().to_lowercase();
    let shown: Vec<usize> = catalog
        .scripts
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            filter.is_empty()
                || s.short.to_lowercase().contains(&filter)
                || s.name.to_lowercase().contains(&filter)
        })
        .map(|(i, _)| i)
        .collect();
    ui.label(RichText::new(format!("CoreScripts ({})", catalog.scripts.len())).strong());
    for i in shown {
        let s = &catalog.scripts[i];
        let entry = entries.get(&s.name);
        let enabled = !entry.is_some_and(|e| e.disabled);
        let selected = panel.selected == Some(Selection::Script(i));
        ui.horizontal(|ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "").on_hover_text("Start this script at Play").changed() {
                *action = Some(Action::ToggleScript {
                    name: s.name.clone(),
                    enable: on,
                });
            }
            if ui.selectable_label(selected, &s.short).clicked() {
                *action = Some(Action::SelectScript(i));
            }
            if entry.is_some_and(|e| e.has_source) {
                ui.label(RichText::new("override").small().color(theme::ACCENT));
            }
            if !enabled {
                ui.label(RichText::new("off").small().color(theme::WARN));
            }
        });
    }
}

fn draw_module_list(
    ui: &mut egui::Ui,
    panel: &mut CoreScriptsPanel,
    catalog: &rustudio_core::core_scripts::Catalog,
    overrides: &HashMap<Vec<String>, Ref>,
    action: &mut Option<Action>,
) {
    let filter = panel.filter.trim().to_lowercase();
    if filter.is_empty() {
        ui.label(RichText::new("CoreGui modules").strong());
        for node in &catalog.modules {
            draw_module_node(ui, panel, node, overrides, action);
        }
    } else {
        let mut matches = Vec::new();
        for node in &catalog.modules {
            collect_matches(node, &filter, &mut matches);
            if matches.len() >= 300 {
                break;
            }
        }
        ui.label(RichText::new(format!("Modules matching “{}”", panel.filter.trim())).strong());
        let hit_limit = matches.len() >= 300;
        for node in matches {
            module_label(ui, panel, node, overrides, action, &node.path.join("."));
        }
        if hit_limit {
            ui.label(RichText::new("Showing the first 300 matches.").small().color(theme::TEXT_DIM));
        }
    }
}

fn draw_module_node(
    ui: &mut egui::Ui,
    panel: &mut CoreScriptsPanel,
    node: &rustudio_core::core_scripts::CatalogModule,
    overrides: &HashMap<Vec<String>, Ref>,
    action: &mut Option<Action>,
) {
    let selected = panel.selected == Some(Selection::Module(node.path.clone()));
    let label = if overrides.contains_key(&node.path) {
        RichText::new(&node.name).color(theme::ACCENT)
    } else {
        RichText::new(&node.name)
    };
    if node.children.is_empty() {
        if ui.selectable_label(selected, label).clicked() {
            *action = Some(Action::SelectModule(node.path.clone()));
        }
        return;
    }
    let id = ui.make_persistent_id(("core_scripts_module", &node.path));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
        .show_header(ui, |ui| {
            if ui.selectable_label(selected, label).clicked() {
                *action = Some(Action::SelectModule(node.path.clone()));
            }
            if overrides.contains_key(&node.path) {
                ui.label(RichText::new("override").small().color(theme::ACCENT));
            }
        })
        .body(|ui| {
            for child in &node.children {
                draw_module_node(ui, panel, child, overrides, action);
            }
        });
}

fn collect_matches<'a>(
    node: &'a rustudio_core::core_scripts::CatalogModule,
    filter: &str,
    out: &mut Vec<&'a rustudio_core::core_scripts::CatalogModule>,
) {
    if out.len() >= 300 {
        return;
    }
    if node.path.join(".").to_lowercase().contains(filter) {
        out.push(node);
    }
    for child in &node.children {
        if out.len() >= 300 {
            return;
        }
        collect_matches(child, filter, out);
    }
}

fn module_label(
    ui: &mut egui::Ui,
    panel: &mut CoreScriptsPanel,
    node: &rustudio_core::core_scripts::CatalogModule,
    overrides: &HashMap<Vec<String>, Ref>,
    action: &mut Option<Action>,
    label: &str,
) {
    let selected = panel.selected == Some(Selection::Module(node.path.clone()));
    let text = if overrides.contains_key(&node.path) {
        RichText::new(label).color(theme::ACCENT)
    } else {
        RichText::new(label)
    };
    if ui.selectable_label(selected, text).clicked() {
        *action = Some(Action::SelectModule(node.path.clone()));
    }
}

fn draw_preview(
    ui: &mut egui::Ui,
    panel: &mut CoreScriptsPanel,
    entries: &HashMap<String, EntryInfo>,
    module_overrides: &HashMap<Vec<String>, Ref>,
    action: &mut Option<Action>,
) {
    let Some(preview) = &panel.preview else {
        ui.add_space(12.0);
        ui.label(
            RichText::new("Select a script or module to read it.")
                .color(theme::TEXT_DIM),
        );
        return;
    };
    ui.heading(&preview.title);
    ui.label(RichText::new(&preview.subtitle).small().color(theme::TEXT_DIM));
    ui.add_space(4.0);

    match &panel.selected {
        Some(Selection::Script(i)) => {
            let script = match &panel.state {
                LoadState::Ready(catalog) => catalog.scripts.get(*i),
                _ => None,
            };
            if let Some(script) = script {
                let entry = entries.get(&script.name);
                let enabled = !entry.is_some_and(|e| e.disabled);
                ui.horizontal(|ui| {
                    let mut on = enabled;
                    if ui.checkbox(&mut on, "Enabled").changed() {
                        *action = Some(Action::ToggleScript {
                            name: script.name.clone(),
                            enable: on,
                        });
                    }
                    if entry.is_some() {
                        if ui.button("Open in editor").clicked() {
                            *action = Some(Action::Open(entry.unwrap().r));
                        }
                        if ui.button("Revert to built-in").clicked() {
                            *action = Some(Action::Revert(entry.unwrap().r));
                        }
                    } else if script.source.is_some() {
                        if ui.button("Copy into this place").clicked() {
                            *action = Some(Action::CopyScript(*i));
                        }
                    } else {
                        ui.label(RichText::new("No Lua source in this install.").color(theme::WARN));
                    }
                });
            }
        }
        Some(Selection::Module(path)) => {
            let entry = module_overrides.get(path);
            ui.horizontal(|ui| {
                if let Some(r) = entry {
                    if ui.button("Open in editor").clicked() {
                        *action = Some(Action::Open(*r));
                    }
                    if ui.button("Revert to built-in").clicked() {
                        *action = Some(Action::Revert(*r));
                    }
                } else if preview.module_copy {
                    if let Some(source) = &preview.source {
                        if ui.button("Copy into this place").clicked() {
                            *action = Some(Action::CopyModule(path.clone(), source.clone()));
                        }
                    } else {
                        ui.label(RichText::new("No Lua source in this install.").color(theme::WARN));
                    }
                } else if preview.source.is_some() {
                    ui.label(
                        RichText::new("Only ModuleScripts can replace a built-in module.")
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                } else {
                    ui.label(RichText::new("No Lua source in this install.").color(theme::WARN));
                }
            });
        }
        None => {}
    }

    ui.separator();
    match &preview.shown {
        Some(source) => {
            if preview.truncated {
                ui.label(
                    RichText::new("Preview cut short. Copy it into the place to see the whole file.")
                        .small()
                        .color(theme::TEXT_DIM),
                );
            }
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(source)
                                .monospace()
                                .size(12.0)
                                .color(Color32::from_rgb(205, 207, 212)),
                        )
                        .wrap(),
                    );
                });
        }
        None => {
            ui.label(
                RichText::new("This install has no Lua file for it; Roblox's compiled bytecode is all Play has.")
                    .color(theme::TEXT_DIM),
            );
        }
    }
}
