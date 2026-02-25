// SPDX-License-Identifier: GPL-3.0-only

use crate::config::Config;
use crate::fl;
use cosmic::app::context_drawer;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::{Length, Subscription};
use cosmic::widget::{self, about::About, menu};
use cosmic::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use pdfium_render::prelude::*;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const APP_ICON: &[u8] = include_bytes!("../resources/icons/hicolor/scalable/apps/icon.svg");

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
pub struct AppModel {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    /// Display a context drawer with the designated page if defined.
    context_page: ContextPage,
    /// The about page for this app.
    about: About,
    /// Key bindings for the application's menu bar.
    key_binds: HashMap<menu::KeyBind, MenuAction>,
    /// Configuration data that persists between application runs.
    config: Config,
    
    /// Global Pdfium bindings instance.
    pdfium: &'static Pdfium,

    /// The currently active PDF state.
    pdf_state: PdfState,
}

pub enum PdfState {
    None,
    Loading,
    Loaded {
        path: PathBuf,
        doc: PdfDocument<'static>,
        current_page_image: Option<cosmic::widget::image::Handle>,
        current_page_index: u16,
        page_count: u16,
        zoom_level: f32,
        search_query: String,
    },
    Error(String),
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    LaunchUrl(String),
    ToggleContextPage(ContextPage),
    UpdateConfig(Config),
    OpenFile,
    FileOpened(Option<PathBuf>),
    NextPage,
    PreviousPage,
    ZoomIn,
    ZoomOut,
    UpdateSearchQuery(String),
    SubmitSearch(String),
}

/// Create a COSMIC application from the app model
impl cosmic::Application for AppModel {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = ();

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "com.github.alfasst.OrbitPDF";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {

        // Create the about widget
        let about = About::default()
            .name(fl!("app-title"))
            .icon(widget::icon::from_svg_bytes(APP_ICON))
            .version(env!("CARGO_PKG_VERSION"))
            .links([(fl!("repository"), REPOSITORY)])
            .license(env!("CARGO_PKG_LICENSE"));

        let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        let exe_dir = exe_path.parent().unwrap_or(&exe_path);
        let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(exe_dir))
            .unwrap_or_else(|_| Pdfium::bind_to_system_library().expect("Could not bind to Pdfium"));
        let pdfium: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));

        // Construct the app model with the runtime's core.
        let mut app = AppModel {
            core,
            context_page: ContextPage::default(),
            about,
            key_binds: HashMap::new(),
            config: cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                .map(|context| match Config::get_entry(&context) {
                    Ok(config) => config,
                    Err((_errors, config)) => config,
                })
                .unwrap_or_default(),
            pdfium,
            pdf_state: PdfState::None,
        };

        // Create a startup command that sets the window title.
        let command = app.update_title();

        (app, command)
    }

    /// Elements to pack at the start of the header bar.
    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        let menu_bar = menu::bar(vec![menu::Tree::with_children(
            menu::root(fl!("view")).apply(Element::from),
            menu::items(
                &self.key_binds,
                vec![menu::Item::Button(fl!("about"), None, MenuAction::About)],
            ),
        )]);

        vec![menu_bar.into()]
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        let mut controls = Vec::new();

        if let PdfState::Loaded { current_page_index, page_count, search_query, .. } = &self.pdf_state {
            let search_input = widget::text_input("Search text...", search_query)
                .on_input(Message::UpdateSearchQuery)
                .on_submit(Message::SubmitSearch)
                .width(Length::Fixed(200.0));
            controls.push(search_input.into());

            let zoom_out_btn = widget::button::icon(widget::icon::from_name("zoom-out-symbolic"))
                .on_press(Message::ZoomOut);
            let zoom_in_btn = widget::button::icon(widget::icon::from_name("zoom-in-symbolic"))
                .on_press(Message::ZoomIn);

            let prev_btn = if *current_page_index > 0 {
                widget::button::icon(widget::icon::from_name("go-previous-symbolic")).on_press(Message::PreviousPage)
            } else {
                widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
            };

            let next_btn = if *current_page_index < *page_count - 1 {
                widget::button::icon(widget::icon::from_name("go-next-symbolic")).on_press(Message::NextPage)
            } else {
                widget::button::icon(widget::icon::from_name("go-next-symbolic"))
            };

            let page_text = widget::text::text(format!("Page {} of {}", current_page_index + 1, page_count));

            let navigation_row = widget::row()
                .push(zoom_out_btn)
                .push(zoom_in_btn)
                .push(prev_btn)
                .push(page_text)
                .push(next_btn)
                .spacing(8)
                .align_y(Vertical::Center);

            controls.push(navigation_row.into());
        }

        let open_btn = widget::button::text("Open PDF").on_press(Message::OpenFile);
        controls.push(open_btn.into());

        controls
    }

    /// Display a context drawer if the context page is requested.
    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<'_, Self::Message>> {
        if !self.core.window.show_context {
            return None;
        }

        Some(match self.context_page {
            ContextPage::About => context_drawer::about(
                &self.about,
                |url| Message::LaunchUrl(url.to_string()),
                Message::ToggleContextPage(ContextPage::About),
            ),
        })
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// Application events will be processed through the view. Any messages emitted by
    /// events received by widgets will be passed to the update method.
    fn view(&self) -> Element<'_, Self::Message> {
        let content: Element<_> = match &self.pdf_state {
            PdfState::None => {
                widget::text::title1("No PDF Opened")
                    .into()
            }
            PdfState::Loading => {
                widget::text::title1("Loading PDF...")
                    .into()
            }
            PdfState::Error(err) => {
                widget::text::title3(format!("Error: {}", err))
                    .into()
            }
            PdfState::Loaded { path, current_page_image, .. } => {
                let mut col = widget::column()
                    .push(widget::text::title3(format!("Loaded: {}", path.to_string_lossy())))
                    .spacing(16)
                    .align_x(Horizontal::Center);
                
                if let Some(handle) = current_page_image {
                    col = col.push(
                        widget::container(
                            widget::image(handle.clone())
                                .width(Length::Fill)
                        )
                        .padding(16)
                    );
                }
                
                widget::scrollable(col).into()
            }
        };

        widget::container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
    }

    /// Register subscriptions for this application.
    ///
    /// Subscriptions are long-running async tasks running in the background which
    /// emit messages to the application through a channel. They can be dynamically
    /// stopped and started conditionally based on application state, or persist
    /// indefinitely.
    fn subscription(&self) -> Subscription<Self::Message> {
        self.core()
            .watch_config::<Config>(Self::APP_ID)
            .map(|update| Message::UpdateConfig(update.config))
    }

    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::OpenFile => {
                let task = tokio::task::spawn_blocking(|| {
                    rfd::FileDialog::new()
                        .set_title("Open PDF")
                        .add_filter("PDF files", &["pdf"])
                        .pick_file()
                });
                
                return Task::perform(
                    async { task.await.unwrap_or(None) },
                    Message::FileOpened,
                )
                .map(Into::into)
            }
            
            Message::FileOpened(path_opt) => {
                if let Some(path) = path_opt {
                    match self.pdfium.load_pdf_from_file(&path, None) {
                        Ok(doc) => {
                            let page_count = doc.pages().len();
                            let current_page_index = 0;
                            let zoom_level = 2.0;

                            let current_page_image = Self::render_page(&doc, current_page_index, zoom_level);

                            self.pdf_state = PdfState::Loaded { 
                                path, doc, current_page_image, current_page_index, page_count, zoom_level, search_query: String::new()
                            };
                        }
                        Err(e) => {
                            self.pdf_state = PdfState::Error(format!("Failed to load PDF: {}", e));
                        }
                    }
                    return self.update_title();
                }
            }

            Message::NextPage => {
                if let PdfState::Loaded { ref doc, ref mut current_page_index, ref mut current_page_image, page_count, zoom_level, .. } = self.pdf_state {
                    if *current_page_index < page_count - 1 {
                        *current_page_index += 1;
                        *current_page_image = Self::render_page(doc, *current_page_index, zoom_level);
                    }
                }
            }
            Message::PreviousPage => {
                if let PdfState::Loaded { ref doc, ref mut current_page_index, ref mut current_page_image, zoom_level, .. } = self.pdf_state {
                    if *current_page_index > 0 {
                        *current_page_index -= 1;
                        *current_page_image = Self::render_page(doc, *current_page_index, zoom_level);
                    }
                }
            }
            Message::ZoomIn => {
                if let PdfState::Loaded { ref doc, current_page_index, ref mut current_page_image, ref mut zoom_level, .. } = self.pdf_state {
                    *zoom_level += 0.2;
                    if *zoom_level > 5.0 { *zoom_level = 5.0; }
                    *current_page_image = Self::render_page(doc, current_page_index, *zoom_level);
                }
            }
            Message::ZoomOut => {
                if let PdfState::Loaded { ref doc, current_page_index, ref mut current_page_image, ref mut zoom_level, .. } = self.pdf_state {
                    *zoom_level -= 0.2;
                    if *zoom_level < 0.2 { *zoom_level = 0.2; }
                    *current_page_image = Self::render_page(doc, current_page_index, *zoom_level);
                }
            }

            Message::UpdateSearchQuery(query) => {
                if let PdfState::Loaded { ref mut search_query, .. } = self.pdf_state {
                    *search_query = query;
                }
            }

            Message::SubmitSearch(_) => {
                if let PdfState::Loaded { ref doc, ref mut current_page_index, ref mut current_page_image, page_count, ref zoom_level, ref search_query, .. } = self.pdf_state {
                    if !search_query.is_empty() {
                        let query_lower = search_query.to_lowercase();
                        // Search pages starting from current + 1, wrapping around
                        let mut found = false;
                        for i in 0..page_count {
                            let check_index = (*current_page_index + 1 + i) % page_count;
                            if let Ok(page) = doc.pages().get(check_index) {
                                if let Ok(text) = page.text() {
                                    let page_str = text.all().to_lowercase();
                                    if page_str.contains(&query_lower) {
                                        *current_page_index = check_index;
                                        *current_page_image = Self::render_page(doc, *current_page_index, *zoom_level);
                                        found = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if !found {
                            println!("cargo:warning=Search matched nothing");
                        }
                    }
                }
            }

            Message::ToggleContextPage(context_page) => {
                if self.context_page == context_page {
                    // Close the context drawer if the toggled context page is the same.
                    self.core.window.show_context = !self.core.window.show_context;
                } else {
                    // Open the context drawer to display the requested context page.
                    self.context_page = context_page;
                    self.core.window.show_context = true;
                }
            }

            Message::UpdateConfig(config) => {
                self.config = config;
            }

            Message::LaunchUrl(url) => match open::that_detached(&url) {
                Ok(()) => {}
                Err(err) => {
                    eprintln!("failed to open {url:?}: {err}");
                }
            },
        }
        Task::none()
    }

}

impl AppModel {
    /// Updates the header and window titles.
    pub fn update_title(&mut self) -> Task<cosmic::Action<Message>> {
        let mut window_title = fl!("app-title");

        if let PdfState::Loaded { path, .. } = &self.pdf_state {
            window_title.push_str(" — ");
            if let Some(name) = path.file_name() {
                window_title.push_str(&name.to_string_lossy());
            }
        }

        if let Some(id) = self.core.main_window_id() {
            self.set_window_title(window_title, id)
        } else {
            Task::none()
        }
    }

    pub fn render_page(doc: &PdfDocument<'static>, index: u16, zoom: f32) -> Option<cosmic::widget::image::Handle> {
        if let Ok(page) = doc.pages().get(index) {
            let config = PdfRenderConfig::new()
                .scale_page_by_factor(zoom)
                .set_clear_color(PdfColor::WHITE);
            if let Ok(bitmap) = page.render_with_config(&config) {
                let img = bitmap.as_image(); // DynamicImage
                let rgba = img.into_rgba8();
                let (width, height) = rgba.dimensions();
                return Some(cosmic::widget::image::Handle::from_rgba(width, height, rgba.into_raw()));
            }
        }
        None
    }
}

/// The context page to display in the context drawer.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum ContextPage {
    #[default]
    About,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuAction {
    About,
}

impl menu::action::MenuAction for MenuAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuAction::About => Message::ToggleContextPage(ContextPage::About),
        }
    }
}
