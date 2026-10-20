use gpui::{
    App, Application, Bounds, Context, TitlebarOptions, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};

struct UnBlockMc;

impl Render for UnBlockMc {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("welcome")
            .tab_group()
            .on_key_down(|event, window, _cx| {
                if event.keystroke.key == "tab" {
                    window.focus_next();
                }
            })
            .size_full()
            .flex()
            .flex_col()
            .justify_center()
            .items_center()
            .gap_4()
            .bg(rgb(0x151b26))
            .text_color(rgb(0xe6edf3))
            .child(div().text_3xl().child("UnBlockMC"))
            .child("Java / .NET binary editor")
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x9baac0))
                    .child("GPUI project initialized. Editing tools are coming next."),
            )
            .child(
                div()
                    .id("repository-link")
                    .tab_index(0)
                    .text_sm()
                    .text_color(rgb(0x58a6ff))
                    .cursor_pointer()
                    .hover(|style| style.underline())
                    .focus(|style| style.underline())
                    .on_click(|_, _, cx| cx.open_url(env!("CARGO_PKG_REPOSITORY")))
                    .child(concat!("GitHub: ", env!("CARGO_PKG_REPOSITORY"))),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(960.0), px(640.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(640.0), px(400.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some("UnBlockMC".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| UnBlockMc),
        )
        .expect("Failed to open the UnBlockMC window");

        cx.activate(true);
    });
}
