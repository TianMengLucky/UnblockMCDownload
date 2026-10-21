#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use gpui::{
    App, Application, Bounds, BoxShadow, ContentMask, Context, Image, ImageFormat, KeyBinding,
    MouseButton, ObjectFit, PathBuilder, PathPromptOptions, RenderImage, TitlebarOptions, Window,
    WindowBounds, WindowControlArea, WindowDecorations, WindowOptions, actions, canvas, div, img,
    point, prelude::*, px, rgb, rgba, size,
};
use std::{path::PathBuf, sync::Arc};
use unblockmcdownload::unlock;

mod consent;
mod window_controls;

actions!(unblockmcdownload, [ChooseFile, StartUnlock, ToggleTaskbar]);

const WALLPAPER: &[u8] = include_bytes!("../assets/wallpaper-app.jpg");

struct UnblockMcDownload {
    icon: Arc<Image>,
    wallpaper: Arc<Image>,
    acrylic: Arc<RenderImage>,
    taskbar_hidden: bool,
    consent: consent::Consent,
    consent_error: Option<String>,
    path: Option<PathBuf>,
    status: String,
    busy: bool,
    modify_auto_update: bool,
    pending_update: Option<String>,
    failed: bool,
    focus: gpui::FocusHandle,
}

impl UnblockMcDownload {
    fn start_update_check(&mut self, cx: &mut Context<Self>) {
        let task = cx.background_executor().spawn(async {
            let version = unblockmcdownload::update::self_update(None)?;
            if version.is_empty() {
                return Ok::<_, String>(None);
            }
            unblockmcdownload::update::self_update(Some(&version)).map(Some)
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Some(path)) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.pending_update = Some(path);
                    this.install_pending_update(cx);
                });
            }
        })
        .detach();
    }

    fn install_pending_update(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Some(path) = self.pending_update.take()
            && unblockmcdownload::update::replace_and_restart(&path).is_ok()
        {
            cx.quit();
        }
    }

    fn accept_consent(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.consent.accept() {
            self.consent_error = Some(format!(
                "无法保存同意记录：{error}。请检查用户目录写入权限后重试。"
            ));
        }
        cx.notify();
    }

    fn toggle_taskbar(&mut self, _: &ToggleTaskbar, window: &mut Window, cx: &mut Context<Self>) {
        match window_controls::set_taskbar_visible(window, self.taskbar_hidden) {
            Ok(()) => self.taskbar_hidden = !self.taskbar_hidden,
            Err(error) => {
                self.status = format!("无法切换任务栏显示：{error}");
                self.failed = true;
            }
        }
        cx.notify();
    }

    fn minimize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.taskbar_hidden {
            if let Err(error) = window_controls::set_taskbar_visible(window, true) {
                self.status = format!("无法恢复任务栏入口：{error}");
                self.failed = true;
                cx.notify();
                return;
            }
            self.taskbar_hidden = false;
            cx.notify();
        }
        window.minimize_window();
    }

    fn choose_file(&mut self, _: &ChooseFile, _: &mut Window, cx: &mut Context<Self>) {
        if !self.consent.accepted || self.busy {
            return;
        }
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择 PCL、HMCL 或 PCL-CE 的 EXE 文件".into()),
        });
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = selection.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            this.path = Some(path);
                            this.status = "文件已选择，点击“开始解锁”自动识别并处理。".into();
                            this.failed = false;
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(err)) => {
                        this.status = format!("无法打开文件选择框：{err}");
                        this.failed = true;
                    }
                    Err(err) => {
                        this.status = format!("文件选择未完成：{err}");
                        this.failed = true;
                    }
                }
                this.install_pending_update(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn start_unlock(&mut self, _: &StartUnlock, _: &mut Window, cx: &mut Context<Self>) {
        if !self.consent.accepted || self.busy {
            return;
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        self.busy = true;
        self.failed = false;
        self.status = "正在识别启动器并移除下载限制，请稍候……".into();
        cx.notify();
        let options = unlock::UnlockOptions {
            modify_auto_update: self.modify_auto_update,
        };
        let task = cx.background_executor().spawn(async move {
            unlock::unlock_with_options(&path, options).map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.status = match result {
                    Ok(outcome) => {
                        let mut message = match outcome.backup {
                            Some(backup) => format!(
                                "已识别为 {}，下载限制已移除。\n原文件备份：{}",
                                outcome.launcher,
                                backup.display()
                            ),
                            None => format!(
                                "已识别为 {}，该文件已解锁，无需重复修改。",
                                outcome.launcher
                            ),
                        };
                        if outcome.auto_update {
                            message.push_str("\n自动更新处理已启用，关闭本工具后仍生效。");
                        }
                        message
                    }
                    Err(error) => {
                        this.failed = true;
                        format!("解锁失败：{error}")
                    }
                };
                this.install_pending_update(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_auto_update(&mut self, cx: &mut Context<Self>) {
        if self.consent.accepted && !self.busy {
            self.modify_auto_update = !self.modify_auto_update;
            cx.notify();
        }
    }

    fn titlebar(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        div()
            .id("custom-titlebar")
            .text_color(rgb(0xf4fff9))
            .font_family("Microsoft YaHei UI")
            .h(px(44.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .pr_3()
            .bg(rgba(0x09282070))
            .child(
                div()
                    .id("titlebar-drag")
                    .flex_1()
                    .h_full()
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(img(self.icon.clone()).size(px(24.)))
                    .child(div().text_sm().child("UnblockMCDownload")),
            )
            .when(cfg!(target_os = "windows"), |bar| {
                bar.child(
                    window_button(
                        "toggle-taskbar",
                        if self.taskbar_hidden {
                            "恢复任务栏"
                        } else {
                            "隐藏任务栏"
                        },
                        WindowIcon::Taskbar,
                        4,
                    )
                    .when(self.taskbar_hidden, |button| button.bg(rgba(0xffffff38)))
                    .hover(|style| style.bg(rgba(0xffffff24)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_taskbar(&ToggleTaskbar, window, cx);
                    })),
                )
            })
            .child(
                window_button("window-minimize", "最小化", WindowIcon::Minimize, 5)
                    .hover(|style| style.bg(rgba(0xffffff24)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.minimize(window, cx);
                    })),
            )
            .child(
                window_button("window-close", "退出", WindowIcon::Close, 6)
                    .hover(|style| style.bg(rgba(0xd94759dd)))
                    .on_click(|_, window, _| window.remove_window()),
            )
    }
}

enum WindowIcon {
    Taskbar,
    Minimize,
    Close,
    Folder,
}

fn window_drag_area() -> gpui::Div {
    div()
        .absolute()
        .inset_0()
        .size_full()
        // Shield focus handlers behind the drag surface from native caption presses.
        .occlude()
        .window_control_area(WindowControlArea::Drag)
        .when(!cfg!(target_os = "windows"), |area| {
            area.on_mouse_down(MouseButton::Left, |_, window, _| {
                window.start_window_move();
            })
        })
}

fn window_icon(icon: WindowIcon) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let strokes: &[&[(f32, f32)]] = match icon {
                WindowIcon::Taskbar => &[
                    &[(2., 3.), (14., 3.), (14., 13.), (2., 13.), (2., 3.)],
                    &[(2., 10.), (14., 10.)],
                    &[(5., 11.5), (11., 11.5)],
                ],
                WindowIcon::Minimize => &[&[(3., 8.), (13., 8.)]],
                WindowIcon::Close => &[&[(3., 3.), (13., 13.)], &[(13., 3.), (3., 13.)]],
                WindowIcon::Folder => &[&[
                    (1.5, 4.),
                    (6., 4.),
                    (8., 6.),
                    (14.5, 6.),
                    (14.5, 13.),
                    (1.5, 13.),
                    (1.5, 4.),
                ]],
            };
            let mut path = PathBuilder::stroke(px(1.25));
            for stroke in strokes {
                path.move_to(bounds.origin + point(px(stroke[0].0), px(stroke[0].1)));
                for &(x, y) in &stroke[1..] {
                    path.line_to(bounds.origin + point(px(x), px(y)));
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, window.text_style().color);
            }
        },
    )
    .size(px(16.))
}

struct WindowButtonTooltip(&'static str);

impl Render for WindowButtonTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(0x193d35))
            .text_color(rgb(0xf4fff9))
            .text_xs()
            .child(self.0)
    }
}

fn window_button(
    id: &'static str,
    label: &'static str,
    icon: WindowIcon,
    tab_index: isize,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .occlude()
        .tab_index(tab_index)
        .w(px(36.))
        .h(px(32.))
        .flex_shrink_0()
        .rounded_md()
        .bg(rgba(0xffffff10))
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .cursor_pointer()
        .border_1()
        .border_color(rgba(0x00000000))
        .focus(|style| style.border_color(rgba(0xffffffaa)))
        .tooltip(move |_, cx| cx.new(|_| WindowButtonTooltip(label)).into())
        .child(window_icon(icon))
}

fn acrylic_backdrop(backdrop: Arc<RenderImage>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let viewport = window.viewport_size();
            let source = backdrop.size(0);
            let width = source.width.0 as f32;
            let height = source.height.0 as f32;
            let scale =
                (f32::from(viewport.width) / width).max(f32::from(viewport.height) / height);
            let painted = size(px(width * scale), px(height * scale));
            let image_bounds = Bounds::new(
                point(
                    (viewport.width - painted.width) / 2.,
                    (viewport.height - painted.height) / 2.,
                ),
                painted,
            );
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                // The tint still provides a readable fallback if the GPU upload fails.
                if let Err(error) =
                    window.paint_image(image_bounds, Default::default(), backdrop.clone(), 0, false)
                {
                    eprintln!("Unable to paint acrylic backdrop: {error}");
                }
            });
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

fn blurred_wallpaper() -> Arc<RenderImage> {
    // Half-resolution blur keeps the same screen-space radius with a quarter of the pixels.
    let wallpaper = image::load_from_memory(WALLPAPER)
        .expect("The embedded wallpaper must be a valid JPEG")
        .into_rgb8();
    let thumbnail = image::imageops::thumbnail(&wallpaper, 400, 250);
    drop(wallpaper);
    let blurred = image::imageops::fast_blur(&thumbnail, 6.);
    let mut pixels = image::RgbaImage::new(blurred.width(), blurred.height());
    for (source, target) in blurred.pixels().zip(pixels.pixels_mut()) {
        target.0 = [source[2], source[1], source[0], 255];
    }
    Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]))
}

impl Render for UnblockMcDownload {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.consent.accepted {
            let notice = div()
                .id("usage-notice")
                .relative()
                .w(px(560.))
                .h(px(480.))
                .max_w_full()
                .max_h_full()
                .overflow_hidden()
                .rounded_xl()
                .border_1()
                .border_color(rgba(0xffffffb0))
                .shadow(vec![BoxShadow {
                    color: rgba(0x082b3c60).into(),
                    offset: point(px(0.), px(12.)),
                    blur_radius: px(32.),
                    spread_radius: px(0.),
                }])
                .text_color(rgb(0x193d35))
                .font_family("Microsoft YaHei UI")
                .child(acrylic_backdrop(self.acrylic.clone()))
                .child(div().absolute().inset_0().size_full().bg(rgba(0xeaf8f1b8)))
                .child(
                    div()
                        .relative()
                        .size_full()
                        .p_6()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(div().text_2xl().child("使用须知"))
                        .child(
                            div()
                                .text_sm()
                                .child("请阅读以下免责声明，同意后方可继续使用。"),
                        )
                        .child(
                            div()
                                .id("disclaimer-text")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .text_sm()
                                .child(self.consent.text.clone()),
                        )
                        .when_some(self.consent_error.clone(), |panel, error| {
                            panel.child(div().text_sm().text_color(rgb(0xb03145)).child(error))
                        })
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_3()
                                .child(
                                    div()
                                        .id("decline-consent")
                                        .occlude()
                                        .tab_index(0)
                                        .focus(|style| style.border_1().border_color(rgb(0x477f70)))
                                        .px_4()
                                        .py_3()
                                        .rounded_md()
                                        .bg(rgb(0xd4e6dd))
                                        .cursor_pointer()
                                        .on_click(|_, window, _| window.remove_window())
                                        .on_key_down(|event, window, cx| {
                                            if matches!(
                                                event.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) {
                                                cx.stop_propagation();
                                                window.remove_window();
                                            }
                                        })
                                        .child("不同意，退出"),
                                )
                                .child(
                                    div()
                                        .id("accept-consent")
                                        .occlude()
                                        .tab_index(1)
                                        .focus(|style| style.border_1().border_color(rgb(0x477f70)))
                                        .px_4()
                                        .py_3()
                                        .rounded_md()
                                        .bg(rgb(0x286e5c))
                                        .text_color(rgb(0xffffff))
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.accept_consent(cx);
                                        }))
                                        .on_key_down(cx.listener(
                                            |this, event: &gpui::KeyDownEvent, _, cx| {
                                                if matches!(
                                                    event.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                ) {
                                                    cx.stop_propagation();
                                                    this.accept_consent(cx);
                                                }
                                            },
                                        ))
                                        .child("同意并继续"),
                                ),
                        ),
                );
            return div()
                .tab_group()
                .on_key_down(|event, window, cx| {
                    if event.keystroke.key == "tab" {
                        if event.keystroke.modifiers.shift {
                            window.focus_prev();
                        } else {
                            window.focus_next();
                        }
                        cx.stop_propagation();
                    }
                })
                .size_full()
                .relative()
                .flex()
                .flex_col()
                .bg(rgb(0x203d36))
                .child(
                    img(self.wallpaper.clone())
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
                .child(div().absolute().inset_0().size_full().bg(rgba(0x09282045)))
                .child(window_drag_area())
                .child(self.titlebar(cx))
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .p_6()
                        .child(notice),
                )
                .into_any_element();
        }
        let can_unlock = self.path.is_some() && !self.busy;
        let path = self
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "尚未选择文件".into());
        div()
            .id("unlock-window")
            .tab_group()
            .on_key_down(|event, window, cx| {
                if event.keystroke.key == "tab" {
                    if event.keystroke.modifiers.shift {
                        window.focus_prev();
                    } else {
                        window.focus_next();
                    }
                    cx.stop_propagation();
                }
            })
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(rgb(0x203d36))
            .text_color(rgb(0xf4fff9))
            .font_family("Microsoft YaHei UI")
            .key_context("UnblockMCDownload")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::choose_file))
            .on_action(cx.listener(Self::start_unlock))
            .on_action(cx.listener(Self::toggle_taskbar))
            .child(
                img(self.wallpaper.clone())
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            )
            .child(window_drag_area())
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(self.titlebar(cx))
                    .child(
                        div()
                            .id("wallpaper-content")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .flex()
                            .items_center()
                            .p_6()
                            .child(
                                div()
                                    .id("operation-panel")
                                    .relative()
                                    .w(px(360.))
                                    .flex_shrink_0()
                                    .p_5()
                                    .border_1()
                                    .border_color(rgba(0xffffffb0))
                                    .shadow(vec![BoxShadow {
                                        color: rgba(0x082b3c50).into(),
                                        offset: point(px(0.), px(12.)),
                                        blur_radius: px(32.),
                                        spread_radius: px(0.),
                                    }])
                                    .text_color(rgb(0x193d35))
                                    .child(acrylic_backdrop(self.acrylic.clone()))
                                    .child(
                                        div()
                                            .absolute()
                                            .inset_0()
                                            .size_full()
                                            .bg(rgba(0xeaf8f1a8)),
                                    )
                                    .child(
                                        div()
                                            .relative()
                                            .flex()
                                            .flex_col()
                                            .gap_3()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_3()
                                                    .child(
                                                        img(self.icon.clone())
                                                            .size(px(56.))
                                                            .flex_shrink_0(),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .flex_col()
                                                            .gap_1()
                                                            .child(div().text_2xl().child("UnblockMCDownload"))
                                                            .child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(rgb(0x507366))
                                                                    .child("PCL · HMCL · PCL-CE 下载解锁"),
                                                            ),
                                                    ),
                                            )
                                            .child(div().mt_2().text_sm().child("目标启动器文件"))
                                            .child(
                                                div()
                                                    .id("choose-file")
                                                    .occlude()
                                                    .tab_index(0)
                                                    .h(px(44.))
                                                    .px_3()
                                                    .flex()
                                                    .items_center()
                                                    .gap_3()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(rgba(0xffffff99))
                                                    .bg(rgba(if self.busy { 0x91ada580 } else { 0xffffff80 }))
                                                    .text_xs()
                                                    .focus(|style| style.border_color(rgb(0x477f70)))
                                                    .when(!self.busy, |field| {
                                                        field.cursor_pointer().hover(|style| style.bg(rgba(0xffffffc0)))
                                                    })
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.choose_file(&ChooseFile, window, cx);
                                                    }))
                                                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                                                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                                            cx.stop_propagation();
                                                            this.choose_file(&ChooseFile, window, cx);
                                                        }
                                                    }))
                                                    .child(
                                                        div()
                                                            .id("selected-path")
                                                            .overflow_x_scroll()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .child(div().whitespace_nowrap().child(path)),
                                                    )
                                                    .tooltip(|_, cx| cx.new(|_| WindowButtonTooltip("选择启动器文件")).into())
                                                    .child(div().flex_shrink_0().text_color(rgb(0x246e64))
                                                        .child(window_icon(WindowIcon::Folder))),
                                            )
                                            .child(
                                                div()
                                                    .id("start-unlock")
                                                    .occlude()
                                                    .tab_index(1)
                                                    .w_full()
                                                    .py_3()
                                                    .rounded_md()
                                                    .text_center()
                                                    .bg(rgba(if can_unlock { 0x286e5cee } else { 0x5c837c50 }))
                                                    .text_color(rgb(if can_unlock { 0xffffff } else { 0x607e72 }))
                                                    .focus(|style| style.border_1().border_color(rgb(0x477f70)))
                                                    .when(can_unlock, |button| {
                                                        button.cursor_pointer().hover(|style| style.bg(rgb(0x358773)))
                                                    })
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.start_unlock(&StartUnlock, window, cx);
                                                    }))
                                                    .child(if self.busy { "处理中…" } else { "开始解锁" }),
                                            )
                                            .child(div().flex().flex_col().gap_1()
                                                .child(div().id("modify-auto-update").occlude().tab_index(2).flex().items_center().gap_3()
                                                    .cursor_pointer().rounded_md().py_2()
                                                    .focus(|style| style.border_1().border_color(rgb(0x477f70)))
                                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_auto_update(cx)))
                                                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                                        if matches!(event.keystroke.key.as_str(), "enter" | "space") { cx.stop_propagation(); this.toggle_auto_update(cx); }
                                                    }))
                                                    .child(div().w(px(40.)).h(px(24.)).rounded_full().flex().items_center().px_1()
                                                        .bg(rgb(if self.modify_auto_update { 0x286e5c } else { 0x91aaa0 }))
                                                        .when(self.modify_auto_update, |style| style.justify_end())
                                                        .child(div().size(px(16.)).rounded_full().bg(rgb(0xffffff))))
                                                    .child("修改自动更新")
                                                    .child(div().text_xs().text_color(rgb(0x507366)).child(if self.modify_auto_update { "已开启" } else { "未开启" })))
                                                .child(div().text_xs().text_color(rgb(0x507366)).child("更新时调用本程序；文件缺失则从 GitHub 下载。")))
                                            .child(
                                                div()
                                                    .min_h(px(64.))
                                                    .text_sm()
                                                    .text_color(rgb(if self.failed { 0xb03145 } else { 0x3d6055 }))
                                                    .child(self.status.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x507366))
                                                    .child("请先关闭启动器。修改前自动备份为 .exe.bak，已有备份不会覆盖。"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x507366))
                                                    .child("Ctrl+O 选择文件 · Ctrl+Enter 开始解锁"),
                                            )
                                            .child(
                                                div()
                                                    .id("repository-link")
                                                    .occlude()
                                                    .tab_index(3)
                                                    .cursor_pointer()
                                                    .hover(|style| style.underline())
                                                    .focus(|style| style.underline())
                                                    .on_click(|_, _, cx| cx.open_url(env!("CARGO_PKG_REPOSITORY")))
                                                    .child(div().text_xs().text_color(rgb(0x246e64))
                                                        .child("GitHub · TianMengLucky / UnblockMCDownload")),
                                            ),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
}

fn main() {
    if let Some(result) =
        unblockmcdownload::update::run_helper(&std::env::args_os().skip(1).collect::<Vec<_>>())
    {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("ctrl-o", ChooseFile, Some("UnblockMCDownload")),
            KeyBinding::new("ctrl-enter", StartUnlock, Some("UnblockMCDownload")),
            KeyBinding::new("ctrl-shift-h", ToggleTaskbar, Some("UnblockMCDownload")),
        ]);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1120.), px(700.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(760.), px(640.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("UnblockMCDownload".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            |window, cx| {
                window_controls::remove_border(window);
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window);
                    let icon = Arc::new(Image::from_bytes(
                        ImageFormat::Png,
                        include_bytes!("../assets/icon-256.png").to_vec(),
                    ));
                    let mut view = UnblockMcDownload {
                        icon,
                        wallpaper: Arc::new(Image::from_bytes(
                            ImageFormat::Jpeg,
                            WALLPAPER.to_vec(),
                        )),
                        acrylic: blurred_wallpaper(),
                        taskbar_hidden: false,
                        consent: consent::Consent::load(),
                        consent_error: None,
                        path: None,
                        status: "选择启动器 EXE 后开始解锁。".into(),
                        busy: false,
                        modify_auto_update: false,
                        pending_update: None,
                        failed: false,
                        focus,
                    };
                    view.start_update_check(cx);
                    view
                })
            },
        )
        .expect("Failed to open the UnblockMCDownload window");
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acrylic_blur_preserves_pixels() {
        let mut expected = image::load_from_memory(WALLPAPER)
            .unwrap()
            .thumbnail(400, 250)
            .fast_blur(6.)
            .to_rgba8();
        for pixel in expected.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        assert_eq!(blurred_wallpaper().as_bytes(0).unwrap(), expected.as_raw());
    }
}
