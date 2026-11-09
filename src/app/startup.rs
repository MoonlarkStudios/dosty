//! Native application startup and Iced configuration.

use super::App;
use iced::Size;
use iced::window;
use tracing_subscriber::EnvFilter;

/// Starts the native editor and runs its Iced event loop.
pub fn run() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "dosty=info".into()))
        .init();

    // Decode the bundled icon before entering the UI event loop.
    let icon = image::load_from_memory(include_bytes!(
        "../../branding/dosty/icons/png/dosty-128.png"
    ))
    .expect("the bundled Dosty icon must be a valid PNG")
    .into_rgba8();
    let icon = window::icon::from_rgba(icon.to_vec(), icon.width(), icon.height())
        .expect("the decoded Dosty icon must contain complete RGBA pixels");

    #[cfg(target_os = "macos")]
    let bootstrap = (std::env::var_os("DOSTY_ACCESSIBILITY_PROTOTYPE").as_deref()
        == Some(std::ffi::OsStr::new("1")))
    .then(super::application::accessibility::Bootstrap::new);
    #[cfg(target_os = "macos")]
    let accessibility = bootstrap.as_ref().map(|bootstrap| bootstrap.config.clone());
    #[cfg(target_os = "macos")]
    let boot = move || {
        let (mut app, task) = App::new_with_accessibility(bootstrap.as_ref());
        let menus = app.install_native_menus();
        (
            app,
            iced::Task::batch([
                task,
                menus,
                iced::Task::done(crate::app::Message::NativeMenusReady),
            ]),
        )
    };
    #[cfg(not(target_os = "macos"))]
    let boot = App::new;

    let application = iced::application(boot, App::update, App::view)
        .default_font(crate::theme::styles::BODY_FONT)
        .font(include_bytes!("../../assets/fonts/Newsreader-Regular.ttf").as_slice())
        .font(include_bytes!("../../assets/fonts/Newsreader-Bold.ttf").as_slice())
        .font(include_bytes!("../../assets/fonts/Newsreader-Italic.ttf").as_slice())
        .font(include_bytes!("../../assets/fonts/Newsreader-BoldItalic.ttf").as_slice())
        .title(App::title)
        .theme(App::theme)
        .style(App::appearance)
        .subscription(App::subscription)
        .window(window::Settings {
            size: Size::new(1180.0, 820.0),
            min_size: Some(Size::new(760.0, 520.0)),
            icon: Some(icon),
            exit_on_close_request: false,
            #[cfg(target_os = "macos")]
            platform_specific: window::settings::PlatformSpecific {
                title_hidden: true,
                titlebar_transparent: true,
                fullsize_content_view: true,
            },
            ..window::Settings::default()
        });
    #[cfg(target_os = "macos")]
    if let Some(accessibility) = accessibility {
        return iced_winit::run_with_accessibility(application, accessibility).map_err(Into::into);
    }
    application.run()
}
