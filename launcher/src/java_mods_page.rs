//! Java mods… on Home's Instance row: choose the folder of mod jars, install or update what the
//! game needs (`horizon_launcher::java_mods`), and have Play start the game with them.
use super::*;
use horizon_launcher::java_mods::{self, State, Status};

pub(super) struct JavaModsPanel {
    status: Status,
    /// Whether the selected version can run Java mods, or why not.
    support: Result<(), String>,
    /// The install under way: what it is doing and how far it is.
    busy: Option<(String, Option<f32>)>,
    /// How the last install ended.
    result: Option<Result<String, String>>,
}

fn support_of(tag: Option<&String>) -> Result<(), String> {
    java_mods::support(
        tag.ok_or_else(|| tr("Install a Horizon version first (Versions).").to_string())?,
    )
}

impl Launcher {
    pub(super) fn toggle_java_mods(&mut self) {
        if self.java_mods.take().is_some() {
            return;
        }
        self.java_mods = Some(JavaModsPanel {
            status: Status::now(),
            support: support_of(self.selected()),
            busy: None,
            result: None,
        });
    }

    fn refresh_java_mods(&mut self) {
        let support = support_of(self.selected());
        if let Some(p) = &mut self.java_mods {
            p.status = Status::now();
            p.support = support;
        }
    }

    fn choose_mod_folder(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("Choose").into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = paths.await {
                if let Some(path) = paths.pop() {
                    let _ = this.update(cx, |this, cx| {
                        let mut config = java_mods::Config::load();
                        config.pack = Some(path);
                        if let Err(e) = config.save() {
                            if let Some(p) = &mut this.java_mods {
                                p.result = Some(Err(e));
                            }
                        }
                        this.refresh_java_mods();
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn install_java_mods(&mut self) {
        let Some(tag) = self.selected().cloned() else {
            return;
        };
        let Some(p) = &mut self.java_mods else {
            return;
        };
        if p.busy.is_some() {
            return;
        }
        p.busy = Some((tr("Starting").to_string(), None));
        p.result = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let step = |label: &str, fraction: Option<f32>| {
                let _ = tx.send(Msg::JavaModsStep(label.to_string(), fraction));
            };
            let done = java_mods::install(&tag, &step).map(|s| s.say());
            let _ = tx.send(Msg::JavaModsDone(done));
        });
    }

    pub(super) fn java_mods_step(&mut self, label: String, fraction: Option<f32>) {
        if let Some(p) = &mut self.java_mods {
            p.busy = Some((label, fraction));
        }
    }

    pub(super) fn java_mods_done(&mut self, result: Result<String, String>) {
        self.refresh_java_mods();
        if let Some(p) = &mut self.java_mods {
            p.busy = None;
            p.result = Some(result);
        }
    }

    fn flip_java_mods(&mut self) {
        let mut config = java_mods::Config::load();
        config.enabled = !config.enabled;
        let saved = config.save();
        self.refresh_java_mods();
        if let (Err(e), Some(p)) = (saved, &mut self.java_mods) {
            p.result = Some(Err(e));
        }
    }

    /// The panel under the Instance row, while open.
    pub(super) fn java_mods_panel(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let t = self.theme;
        let p = self.java_mods.as_ref()?;
        let status = p.status.clone();
        let busy = p.busy.clone();
        let result = p.result.clone();
        let support = p.support.clone();
        let ready = status.state == State::Ready;
        let folder = match &status.pack {
            Some(path) => text(theme::LABEL)
                .font_family(theme::MONO)
                .text_color(t.ink_soft)
                .child(path.display().to_string()),
            None => text(theme::LABEL)
                .text_color(t.muted)
                .child(tr("No folder chosen")),
        };
        let choose = button(
            "java-mods-choose",
            tr("Choose folder…"),
            Kind::Secondary,
            &t,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.choose_mod_folder(cx);
        }));
        let can_install = busy.is_none()
            && support.is_ok()
            && !matches!(status.state, State::NoPack | State::EmptyPack);
        let mut install = button(
            "java-mods-install",
            if ready { tr("Update") } else { tr("Install") },
            if ready {
                Kind::Secondary
            } else {
                Kind::Primary
            },
            &t,
        );
        if can_install {
            install = install.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.install_java_mods();
                cx.notify();
            }));
        } else {
            install = install.opacity(0.5);
        }
        let play_with = div()
            .id("java-mods-play")
            .flex()
            .items_center()
            .gap(px(10.))
            .cursor_pointer()
            .child(switch("java-mods-play-s", status.enabled, &t))
            .child(
                text(theme::LABEL)
                    .text_color(t.ink_soft)
                    .child(tr("Play with these mods")),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.flip_java_mods();
                cx.notify();
            }));
        let note = |s: String, color: Hsla| text(theme::CAPTION).text_color(color).child(s);
        Some(
            plate(&t)
                .flex()
                .flex_col()
                .gap(px(12.))
                .px(px(14.))
                .py(px(12.))
                .ml(px(40.))
                .child(caps(tr("Java mods")).text_color(t.muted))
                .child(note(
                    tr("A folder of NeoForge 26.3 mod jars (Mekanism, Applied Energistics 2...). The launcher gets Java 25 and NeoForge's libraries for them; the first start of the game waits on the loading screen while the mods' blocks are made.").to_string(),
                    t.muted,
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(choose)
                        .child(folder),
                )
                .child(note(status.say(), t.ink_soft))
                .children(support.err().map(|e| note(e, t.danger)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(install)
                        .children(busy.as_ref().map(|(label, _)| note(label.clone(), t.muted))),
                )
                .children(busy.as_ref().map(|(_, f)| {
                    div()
                        .flex()
                        .w(px(520.))
                        .child(progress_bar(f.map(|f| (0., f)), &t))
                }))
                .children(result.map(|r| match r {
                    Ok(said) => note(said, t.muted),
                    Err(e) => note(e, t.danger),
                }))
                .child(play_with)
                .child(note(
                    tr("Servers: a server's Settings tab has the same switch. Players joining need the same mods installed.").to_string(),
                    t.muted,
                )),
        )
    }
}
