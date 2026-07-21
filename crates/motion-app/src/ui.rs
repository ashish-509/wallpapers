use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use anyhow::{Context, Result};
use gtk::{gdk, gio, glib};
use motion_service::{Catalog, CatalogItem, Playlist, RotationSettings};
use num_traits::ToPrimitive;

use crate::operations::{self, PreparedMedia};
use crate::paths::AppPaths;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ViewMode {
    Discover,
    Library,
    Favorites,
    Playlist,
}

#[derive(Clone)]
struct Widgets {
    window: adw::ApplicationWindow,
    toast_overlay: adw::ToastOverlay,
    search: gtk::SearchEntry,
    spinner: gtk::Spinner,
    grid: gtk::FlowBox,
    grid_stack: gtk::Stack,
    split: adw::OverlaySplitView,
    discover_toggle: gtk::ToggleButton,
    library_toggle: gtk::ToggleButton,
    favorites_toggle: gtk::ToggleButton,
    playlist_toggle: gtk::ToggleButton,
    detail_title: gtk::Label,
    detail_author: gtk::Label,
    detail_categories: gtk::Label,
    detail_license: gtk::Label,
    source_link: gtk::LinkButton,
    preview_stack: gtk::Stack,
    preview_image: gtk::Picture,
    preview_video: gtk::Picture,
    media_controls: gtk::MediaControls,
    favorite_button: gtk::Button,
    download_button: gtk::Button,
    apply_button: gtk::Button,
    playlist_dropdown: gtk::DropDown,
    add_playlist_button: gtk::Button,
}

struct HeaderSection {
    header: adw::HeaderBar,
    search_row: gtk::Box,
    search: gtk::SearchEntry,
    spinner: gtk::Spinner,
    refresh_button: gtk::Button,
    settings_button: gtk::Button,
    discover_toggle: gtk::ToggleButton,
    library_toggle: gtk::ToggleButton,
    favorites_toggle: gtk::ToggleButton,
    playlist_toggle: gtk::ToggleButton,
}

struct BrowserSection {
    root: gtk::Stack,
    grid: gtk::FlowBox,
}

struct DetailSection {
    root: gtk::ScrolledWindow,
    back_button: gtk::Button,
    detail_title: gtk::Label,
    detail_author: gtk::Label,
    detail_categories: gtk::Label,
    detail_license: gtk::Label,
    source_link: gtk::LinkButton,
    preview_stack: gtk::Stack,
    preview_image: gtk::Picture,
    preview_video: gtk::Picture,
    media_controls: gtk::MediaControls,
    favorite_button: gtk::Button,
    download_button: gtk::Button,
    apply_button: gtk::Button,
    playlist_dropdown: gtk::DropDown,
    add_playlist_button: gtk::Button,
    create_playlist_button: gtk::Button,
}

struct AppState {
    catalog: Rc<Catalog>,
    paths: AppPaths,
    runtime: Arc<tokio::runtime::Runtime>,
    settings: Option<gio::Settings>,
    items: RefCell<Vec<CatalogItem>>,
    playlists: RefCell<Vec<Playlist>>,
    selected: Cell<Option<usize>>,
    mode: Cell<ViewMode>,
    media: RefCell<Option<gtk::MediaFile>>,
    widgets: Widgets,
}

pub fn build(application: &adw::Application) -> Result<()> {
    let paths = AppPaths::create()?;
    let catalog = Rc::new(Catalog::open(&paths.database)?);
    operations::reconcile_cache(&catalog)?;
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?,
    );
    install_css();

    let widgets = build_widgets(application, operations::settings().is_some());
    let state = Rc::new(AppState {
        catalog,
        paths,
        runtime,
        settings: operations::settings(),
        items: RefCell::new(Vec::new()),
        playlists: RefCell::new(Vec::new()),
        selected: Cell::new(None),
        mode: Cell::new(ViewMode::Discover),
        media: RefCell::new(None),
        widgets,
    });
    connect_actions(&state);
    reload_playlists(&state);
    refresh_grid(&state, None);
    state.widgets.window.present();
    Ok(())
}

fn build_widgets(application: &adw::Application, settings_available: bool) -> Widgets {
    let header = build_header_section();
    let browser = build_browser_section();
    let details = build_detail_section();

    let split = adw::OverlaySplitView::new();
    split.set_content(Some(&browser.root));
    split.set_sidebar(Some(&details.root));
    split.set_sidebar_position(gtk::PackType::End);
    split.set_min_sidebar_width(360.0);
    split.set_max_sidebar_width(480.0);
    split.set_sidebar_width_fraction(0.36);
    split.set_pin_sidebar(true);
    split.set_show_sidebar(true);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    if !settings_available {
        let banner = adw::Banner::new("Install the Motion Shell extension to apply wallpapers");
        banner.set_revealed(true);
        content.append(&banner);
    }
    content.append(&header.search_row);
    content.append(&split);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header.header);
    toolbar.set_content(Some(&content));
    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&toolbar));
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("Motion")
        .default_width(1220)
        .default_height(780)
        .content(&toast_overlay)
        .build();

    let breakpoint = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 850px")
            .expect("the adaptive breakpoint is a valid constant"),
    );
    breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
    breakpoint.add_setter(&split, "show-sidebar", Some(&false.to_value()));
    breakpoint.add_setter(&details.back_button, "visible", Some(&true.to_value()));
    window.add_breakpoint(breakpoint);

    header.refresh_button.connect_clicked({
        let search = header.search.clone();
        move |_| search.emit_activate()
    });
    header.settings_button.connect_clicked({
        let window = window.clone();
        move |_| {
            gtk::prelude::WidgetExt::activate_action(&window, "win.settings", None).ok();
        }
    });
    details.create_playlist_button.connect_clicked({
        let window = window.clone();
        move |_| {
            gtk::prelude::WidgetExt::activate_action(&window, "win.new-playlist", None).ok();
        }
    });
    details.back_button.connect_clicked({
        let split = split.clone();
        move |_| split.set_show_sidebar(false)
    });

    Widgets {
        window,
        toast_overlay,
        search: header.search,
        spinner: header.spinner,
        grid: browser.grid,
        grid_stack: browser.root,
        split,
        discover_toggle: header.discover_toggle,
        library_toggle: header.library_toggle,
        favorites_toggle: header.favorites_toggle,
        playlist_toggle: header.playlist_toggle,
        detail_title: details.detail_title,
        detail_author: details.detail_author,
        detail_categories: details.detail_categories,
        detail_license: details.detail_license,
        source_link: details.source_link,
        preview_stack: details.preview_stack,
        preview_image: details.preview_image,
        preview_video: details.preview_video,
        media_controls: details.media_controls,
        favorite_button: details.favorite_button,
        download_button: details.download_button,
        apply_button: details.apply_button,
        playlist_dropdown: details.playlist_dropdown,
        add_playlist_button: details.add_playlist_button,
    }
}

fn build_header_section() -> HeaderSection {
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search Wikimedia Commons")
        .hexpand(true)
        .width_request(320)
        .build();
    let spinner = gtk::Spinner::new();
    spinner.set_visible(false);
    let refresh_button = icon_button("view-refresh-symbolic", "Refresh search");

    let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    search_row.set_margin_start(18);
    search_row.set_margin_end(18);
    search_row.set_margin_top(12);
    search_row.set_margin_bottom(12);
    search_row.append(&search);
    search_row.append(&spinner);
    search_row.append(&refresh_button);

    let discover_toggle = gtk::ToggleButton::with_label("Discover");
    let library_toggle = gtk::ToggleButton::with_label("Library");
    let favorites_toggle = gtk::ToggleButton::with_label("Favorites");
    let playlist_toggle = gtk::ToggleButton::with_label("Playlists");
    library_toggle.set_group(Some(&discover_toggle));
    favorites_toggle.set_group(Some(&discover_toggle));
    playlist_toggle.set_group(Some(&discover_toggle));
    discover_toggle.set_active(true);
    let mode_switcher = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    mode_switcher.add_css_class("linked");
    mode_switcher.add_css_class("mode-switcher");
    for button in [
        &discover_toggle,
        &library_toggle,
        &favorites_toggle,
        &playlist_toggle,
    ] {
        mode_switcher.append(button);
    }

    let settings_button = icon_button("preferences-system-symbolic", "Settings");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Motion", "Live wallpapers")));
    header.pack_start(&mode_switcher);
    header.pack_end(&settings_button);

    HeaderSection {
        header,
        search_row,
        search,
        spinner,
        refresh_button,
        settings_button,
        discover_toggle,
        library_toggle,
        favorites_toggle,
        playlist_toggle,
    }
}

fn build_browser_section() -> BrowserSection {
    let grid = gtk::FlowBox::builder()
        .column_spacing(14)
        .row_spacing(14)
        .homogeneous(true)
        .max_children_per_line(3)
        .min_children_per_line(1)
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    grid.set_margin_start(18);
    grid.set_margin_end(18);
    grid.set_margin_bottom(18);
    let grid_scroll = gtk::ScrolledWindow::builder()
        .child(&grid)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_width(520)
        .build();
    let empty = adw::StatusPage::builder()
        .icon_name("video-x-generic-symbolic")
        .title("No wallpapers here")
        .description("Search Commons or choose another collection")
        .build();
    let grid_stack = gtk::Stack::new();
    grid_stack.add_named(&grid_scroll, Some("grid"));
    grid_stack.add_named(&empty, Some("empty"));

    BrowserSection {
        root: grid_stack,
        grid,
    }
}

fn build_preview() -> (gtk::Stack, gtk::Picture, gtk::Picture, gtk::MediaControls) {
    let preview_image = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Cover)
        .can_shrink(true)
        .build();
    let preview_video = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Cover)
        .can_shrink(true)
        .build();
    let preview_empty = gtk::Image::builder()
        .icon_name("video-x-generic-symbolic")
        .pixel_size(64)
        .build();
    preview_empty.add_css_class("dim-label");
    let preview_stack = gtk::Stack::builder()
        .width_request(320)
        .height_request(180)
        .build();
    preview_stack.add_named(&preview_image, Some("image"));
    preview_stack.add_named(&preview_video, Some("video"));
    preview_stack.add_named(&preview_empty, Some("empty"));
    preview_stack.add_css_class("preview-surface");

    let media_controls = gtk::MediaControls::new(gtk::MediaStream::NONE);
    media_controls.set_visible(false);
    (preview_stack, preview_image, preview_video, media_controls)
}

fn build_detail_section() -> DetailSection {
    let (preview_stack, preview_image, preview_video, media_controls) = build_preview();
    let back_button = icon_button("go-previous-symbolic", "Back to wallpapers");
    back_button.set_visible(false);
    let detail_title = gtk::Label::builder()
        .label("Select a wallpaper")
        .xalign(0.0)
        .wrap(true)
        .css_classes(["title-2"])
        .build();
    let detail_author = metadata_label();
    let detail_categories = metadata_label();
    let detail_license = metadata_label();
    let source_link = gtk::LinkButton::with_label("https://commons.wikimedia.org", "View source");
    source_link.set_halign(gtk::Align::Start);

    let favorite_button = text_button("non-starred-symbolic", "Favorite");
    let download_button = text_button("folder-download-symbolic", "Download");
    let apply_button = text_button("media-playback-start-symbolic", "Apply");
    apply_button.add_css_class("suggested-action");
    for button in [&favorite_button, &download_button, &apply_button] {
        button.set_sensitive(false);
    }
    let action_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    action_row.append(&favorite_button);
    action_row.append(&download_button);
    action_row.append(&apply_button);

    let playlist_dropdown = gtk::DropDown::from_strings(&["Choose playlist"]);
    playlist_dropdown.set_hexpand(true);
    let add_playlist_button = icon_button("list-add-symbolic", "Add to playlist");
    add_playlist_button.set_sensitive(false);
    let create_playlist_button = icon_button("folder-new-symbolic", "New playlist");
    let playlist_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    playlist_row.append(&playlist_dropdown);
    playlist_row.append(&add_playlist_button);
    playlist_row.append(&create_playlist_button);

    let details = gtk::Box::new(gtk::Orientation::Vertical, 12);
    details.set_margin_start(20);
    details.set_margin_end(20);
    details.set_margin_top(18);
    details.set_margin_bottom(18);
    details.add_css_class("detail-pane");
    details.append(&back_button);
    details.append(&preview_stack);
    details.append(&media_controls);
    details.append(&detail_title);
    details.append(&detail_author);
    details.append(&detail_categories);
    details.append(&detail_license);
    details.append(&source_link);
    details.append(&action_row);
    details.append(&playlist_row);
    let detail_scroll = gtk::ScrolledWindow::builder()
        .child(&details)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();

    DetailSection {
        root: detail_scroll,
        back_button,
        detail_title,
        detail_author,
        detail_categories,
        detail_license,
        source_link,
        preview_stack,
        preview_image,
        preview_video,
        media_controls,
        favorite_button,
        download_button,
        apply_button,
        playlist_dropdown,
        add_playlist_button,
        create_playlist_button,
    }
}

fn connect_actions(state: &Rc<AppState>) {
    state.widgets.search.connect_activate({
        let state = Rc::clone(state);
        move |entry| run_search(&state, entry.text().as_str())
    });
    state.widgets.search.connect_search_changed({
        let state = Rc::clone(state);
        move |_| refresh_grid(&state, None)
    });
    state.widgets.grid.connect_child_activated({
        let state = Rc::clone(state);
        move |_, child| {
            select_item(&state, child.index().try_into().ok());
            if state.widgets.split.is_collapsed() {
                state.widgets.split.set_show_sidebar(true);
            }
        }
    });

    connect_mode(state, &state.widgets.discover_toggle, ViewMode::Discover);
    connect_mode(state, &state.widgets.library_toggle, ViewMode::Library);
    connect_mode(state, &state.widgets.favorites_toggle, ViewMode::Favorites);
    connect_mode(state, &state.widgets.playlist_toggle, ViewMode::Playlist);

    state.widgets.favorite_button.connect_clicked({
        let state = Rc::clone(state);
        move |_| toggle_favorite(&state)
    });
    state.widgets.download_button.connect_clicked({
        let state = Rc::clone(state);
        move |_| download_selected(&state)
    });
    state.widgets.apply_button.connect_clicked({
        let state = Rc::clone(state);
        move |_| apply_selected(&state)
    });
    state.widgets.add_playlist_button.connect_clicked({
        let state = Rc::clone(state);
        move |_| add_selected_to_playlist(&state)
    });
    state.widgets.playlist_dropdown.connect_selected_notify({
        let state = Rc::clone(state);
        move |_| {
            state.widgets.add_playlist_button.set_sensitive(
                state.selected.get().is_some() && state.widgets.playlist_dropdown.selected() > 0,
            );
            if state.mode.get() == ViewMode::Playlist {
                refresh_grid(&state, None);
            }
        }
    });

    let settings_action = gio::SimpleAction::new("settings", None);
    settings_action.connect_activate({
        let state = Rc::clone(state);
        move |_, _| show_settings(&state)
    });
    state.widgets.window.add_action(&settings_action);

    let playlist_action = gio::SimpleAction::new("new-playlist", None);
    playlist_action.connect_activate({
        let state = Rc::clone(state);
        move |_, _| show_new_playlist(&state)
    });
    state.widgets.window.add_action(&playlist_action);
}

fn connect_mode(state: &Rc<AppState>, button: &gtk::ToggleButton, mode: ViewMode) {
    button.connect_toggled({
        let state = Rc::clone(state);
        move |button| {
            if button.is_active() {
                state.mode.set(mode);
                refresh_grid(&state, None);
            }
        }
    });
}

fn refresh_grid(state: &Rc<AppState>, preferred_id: Option<&str>) {
    while let Some(child) = state.widgets.grid.first_child() {
        state.widgets.grid.remove(&child);
    }
    let query = state.widgets.search.text();
    let result = match state.mode.get() {
        ViewMode::Playlist => selected_playlist(state).map_or_else(
            || Ok(Vec::new()),
            |playlist| state.catalog.playlist_items(playlist.id),
        ),
        ViewMode::Discover | ViewMode::Library | ViewMode::Favorites => state
            .catalog
            .list(&query, state.mode.get() == ViewMode::Favorites),
    };
    let mut items = match result {
        Ok(items) => items,
        Err(error) => {
            toast(state, &format!("Library error: {error}"));
            Vec::new()
        }
    };
    if state.mode.get() == ViewMode::Library {
        items.retain(|item| item.local_path.is_some());
    }
    for item in &items {
        state.widgets.grid.insert(&wallpaper_card(item), -1);
    }
    state
        .widgets
        .grid_stack
        .set_visible_child_name(if items.is_empty() { "empty" } else { "grid" });
    let selected = preferred_id
        .and_then(|id| items.iter().position(|item| item.id == id))
        .or((!items.is_empty()).then_some(0));
    state.items.replace(items);
    select_item(state, selected);
}

fn wallpaper_card(item: &CatalogItem) -> gtk::FlowBoxChild {
    let media = gtk::Stack::builder()
        .width_request(220)
        .height_request(124)
        .build();
    if let Some(path) = item.thumbnail_path.as_deref() {
        let picture = gtk::Picture::for_filename(path);
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_can_shrink(true);
        media.add_named(&picture, Some("image"));
    } else {
        let icon = gtk::Image::builder()
            .icon_name("video-x-generic-symbolic")
            .pixel_size(48)
            .build();
        icon.add_css_class("dim-label");
        media.add_named(&icon, Some("icon"));
    }
    let title = gtk::Label::builder()
        .label(&item.wallpaper.title)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["heading"])
        .build();
    let author = gtk::Label::builder()
        .label(&item.wallpaper.author)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["dim-label", "caption"])
        .build();
    let copy = gtk::Box::new(gtk::Orientation::Vertical, 3);
    copy.add_css_class("tile-copy");
    copy.append(&title);
    copy.append(&author);
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("wallpaper-card");
    card.append(&media);
    card.append(&copy);
    let child = gtk::FlowBoxChild::new();
    child.set_child(Some(&card));
    child
}

fn select_item(state: &Rc<AppState>, index: Option<usize>) {
    state.selected.set(index);
    let Some(item) = index.and_then(|index| state.items.borrow().get(index).cloned()) else {
        clear_details(state);
        return;
    };
    if let Some(media) = state.media.borrow_mut().take() {
        media.pause();
    }
    state.widgets.detail_title.set_label(&item.wallpaper.title);
    state
        .widgets
        .detail_author
        .set_label(&format!("By {}", item.wallpaper.author));
    state.widgets.detail_categories.set_label(
        &item
            .wallpaper
            .categories
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join(" · "),
    );
    state
        .widgets
        .detail_license
        .set_label(&item.wallpaper.license.name);
    state
        .widgets
        .source_link
        .set_uri(&item.wallpaper.description_url);
    state.widgets.source_link.set_visible(true);
    state.widgets.favorite_button.set_sensitive(true);
    state.widgets.favorite_button.set_label(if item.favorite {
        "Favorited"
    } else {
        "Favorite"
    });
    state
        .widgets
        .favorite_button
        .set_icon_name(if item.favorite {
            "starred-symbolic"
        } else {
            "non-starred-symbolic"
        });
    state
        .widgets
        .download_button
        .set_sensitive(item.local_path.is_none());
    state
        .widgets
        .download_button
        .set_label(if item.local_path.is_some() {
            "Downloaded"
        } else {
            "Download"
        });
    state
        .widgets
        .apply_button
        .set_sensitive(item.local_path.is_some() && state.settings.is_some());
    state
        .widgets
        .add_playlist_button
        .set_sensitive(state.widgets.playlist_dropdown.selected() > 0);

    if let Some(path) = item.local_path.as_deref() {
        let media = gtk::MediaFile::for_filename(path);
        media.set_loop(true);
        media.set_muted(true);
        state.widgets.preview_video.set_paintable(Some(&media));
        state.widgets.media_controls.set_media_stream(Some(&media));
        state.widgets.media_controls.set_visible(true);
        state.widgets.preview_stack.set_visible_child_name("video");
        media.play();
        state.media.replace(Some(media));
    } else if let Some(path) = item.thumbnail_path.as_deref() {
        state.widgets.preview_image.set_filename(Some(path));
        state.widgets.media_controls.set_visible(false);
        state.widgets.preview_stack.set_visible_child_name("image");
    } else {
        state.widgets.media_controls.set_visible(false);
        state.widgets.preview_stack.set_visible_child_name("empty");
    }
}

fn clear_details(state: &AppState) {
    if let Some(media) = state.media.borrow_mut().take() {
        media.pause();
    }
    state.widgets.detail_title.set_label("Select a wallpaper");
    for label in [
        &state.widgets.detail_author,
        &state.widgets.detail_categories,
        &state.widgets.detail_license,
    ] {
        label.set_label("");
    }
    state.widgets.source_link.set_visible(false);
    state.widgets.preview_stack.set_visible_child_name("empty");
    state.widgets.media_controls.set_visible(false);
    for button in [
        &state.widgets.favorite_button,
        &state.widgets.download_button,
        &state.widgets.apply_button,
        &state.widgets.add_playlist_button,
    ] {
        button.set_sensitive(false);
    }
}

fn run_search(state: &Rc<AppState>, query: &str) {
    let query = query.trim().to_owned();
    if query.is_empty() {
        toast(state, "Enter a search term");
        return;
    }
    set_busy(state, true);
    state.widgets.discover_toggle.set_active(true);
    let handle = state.runtime.spawn(operations::discover(
        query.clone(),
        state.paths.previews.clone(),
    ));
    let state = Rc::clone(state);
    glib::MainContext::default().spawn_local(async move {
        let result = handle
            .await
            .context("search task stopped")
            .and_then(|result| result);
        set_busy(&state, false);
        match result {
            Ok(results) => {
                let count = results.len();
                if let Err(error) = operations::persist_search(
                    &state.catalog,
                    &query,
                    results,
                    operations::now_epoch(),
                ) {
                    toast(&state, &format!("Could not save results: {error}"));
                    return;
                }
                refresh_grid(&state, None);
                toast(&state, &format!("Found {count} reusable wallpapers"));
            }
            Err(error) => toast(&state, &format!("Search failed: {error:#}")),
        }
    });
}

fn download_selected(state: &Rc<AppState>) {
    let Some(item) = selected_item(state) else {
        return;
    };
    set_busy(state, true);
    state.widgets.download_button.set_sensitive(false);
    let id = item.id.clone();
    let handle = state
        .runtime
        .spawn(operations::prepare(item.wallpaper, state.paths.clone()));
    let state = Rc::clone(state);
    glib::MainContext::default().spawn_local(async move {
        let result = handle
            .await
            .context("download task stopped")
            .and_then(|result| result);
        set_busy(&state, false);
        match result {
            Ok(media) => finish_download(&state, &id, &media),
            Err(error) => {
                toast(&state, &format!("Download failed: {error:#}"));
                refresh_grid(&state, Some(&id));
            }
        }
    });
}

fn finish_download(state: &Rc<AppState>, id: &str, media: &PreparedMedia) {
    let saved = state.catalog.record_download(
        id,
        &media.video,
        &media.thumbnail,
        media.bytes,
        operations::now_epoch(),
    );
    if let Err(error) = saved {
        toast(state, &format!("Could not save download: {error}"));
        return;
    }
    let limit_gb = state
        .settings
        .as_ref()
        .map_or(10, |settings| settings.uint("cache-limit-gb"));
    if let Err(error) =
        operations::enforce_cache_limit(&state.catalog, u64::from(limit_gb) * 1024 * 1024 * 1024)
    {
        toast(state, &format!("Cache cleanup failed: {error}"));
    }
    refresh_grid(state, Some(id));
    toast(state, "Wallpaper is ready");
}

fn apply_selected(state: &Rc<AppState>) {
    let (Some(item), Some(settings)) = (selected_item(state), state.settings.as_ref()) else {
        toast(
            state,
            "Install the Shell extension before applying wallpapers",
        );
        return;
    };
    match operations::apply(&state.catalog, settings, &item, operations::now_epoch()) {
        Ok(()) => toast(state, "Wallpaper applied"),
        Err(error) => toast(state, &format!("Could not apply wallpaper: {error}")),
    }
}

fn toggle_favorite(state: &Rc<AppState>) {
    let Some(item) = selected_item(state) else {
        return;
    };
    if let Err(error) = state.catalog.set_favorite(&item.id, !item.favorite) {
        toast(state, &format!("Could not update favorite: {error}"));
        return;
    }
    refresh_grid(state, Some(&item.id));
}

fn add_selected_to_playlist(state: &Rc<AppState>) {
    let (Some(item), Some(playlist)) = (selected_item(state), selected_playlist(state)) else {
        return;
    };
    match state.catalog.add_to_playlist(playlist.id, &item.id) {
        Ok(()) => toast(state, &format!("Added to {}", playlist.name)),
        Err(error) => toast(state, &format!("Could not update playlist: {error}")),
    }
}

fn reload_playlists(state: &Rc<AppState>) {
    match state.catalog.playlists() {
        Ok(playlists) => {
            let mut names = vec!["Choose playlist".to_owned()];
            names.extend(playlists.iter().map(|playlist| playlist.name.clone()));
            let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
            state
                .widgets
                .playlist_dropdown
                .set_model(Some(&gtk::StringList::new(&refs)));
            state.playlists.replace(playlists);
        }
        Err(error) => toast(state, &format!("Could not load playlists: {error}")),
    }
}

fn show_new_playlist(state: &Rc<AppState>) {
    let entry = gtk::Entry::builder()
        .placeholder_text("Playlist name")
        .activates_default(true)
        .build();
    let dialog = adw::AlertDialog::builder()
        .heading("New Playlist")
        .body("Name this wallpaper collection")
        .extra_child(&entry)
        .default_response("create")
        .close_response("cancel")
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("create", "Create");
    dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
    dialog.connect_response(Some("create"), {
        let state = Rc::clone(state);
        move |_, _| match state.catalog.create_playlist(entry.text().as_str()) {
            Ok(_) => {
                reload_playlists(&state);
                toast(&state, "Playlist created");
            }
            Err(error) => toast(&state, &format!("Could not create playlist: {error}")),
        }
    });
    dialog.present(Some(&state.widgets.window));
}

fn show_settings(state: &Rc<AppState>) {
    let Some(settings) = state.settings.clone() else {
        toast(state, "The Motion settings schema is not installed");
        return;
    };
    let dialog = adw::PreferencesDialog::builder()
        .title("Motion Settings")
        .content_width(620)
        .content_height(680)
        .build();
    let page = adw::PreferencesPage::builder()
        .title("Settings")
        .icon_name("preferences-system-symbolic")
        .build();

    page.add(&build_pause_group(&settings));
    page.add(&build_resource_group(&settings));
    page.add(&build_rotation_group(state));
    dialog.add(&page);
    dialog.present(Some(&state.widgets.window));
}

fn build_pause_group(settings: &gio::Settings) -> adw::PreferencesGroup {
    let pause_group = adw::PreferencesGroup::builder()
        .title("Pause playback")
        .build();
    for (key, title, subtitle) in [
        (
            "pause-fullscreen",
            "Fullscreen apps",
            "Resume when fullscreen closes",
        ),
        ("pause-game-mode", "GameMode", "Yield resources to games"),
        (
            "pause-power-saver",
            "Power saver",
            "Respect the system power profile",
        ),
        (
            "pause-low-battery",
            "Low battery",
            "Pause below the battery threshold",
        ),
    ] {
        let row = adw::SwitchRow::builder()
            .title(title)
            .subtitle(subtitle)
            .build();
        settings.bind(key, &row, "active").build();
        pause_group.add(&row);
    }
    let battery = spin_row(
        "Battery threshold",
        5.0,
        50.0,
        f64::from(settings.uint("low-battery-threshold")),
    );
    battery.connect_value_notify({
        let settings = settings.clone();
        move |row| {
            settings
                .set_uint("low-battery-threshold", spin_value(row))
                .ok();
        }
    });
    pause_group.add(&battery);
    pause_group
}

fn build_resource_group(settings: &gio::Settings) -> adw::PreferencesGroup {
    let resource_group = adw::PreferencesGroup::builder()
        .title("Storage and updates")
        .build();
    let cache = spin_row(
        "Cache limit (GB)",
        1.0,
        100.0,
        f64::from(settings.uint("cache-limit-gb")),
    );
    cache.connect_value_notify({
        let settings = settings.clone();
        move |row| {
            settings.set_uint("cache-limit-gb", spin_value(row)).ok();
        }
    });
    resource_group.add(&cache);
    let refresh = spin_row(
        "Refresh interval (hours)",
        1.0,
        168.0,
        f64::from(settings.uint("refresh-hours")),
    );
    refresh.connect_value_notify({
        let settings = settings.clone();
        move |row| {
            settings.set_uint("refresh-hours", spin_value(row)).ok();
        }
    });
    resource_group.add(&refresh);
    resource_group
}

fn build_rotation_group(state: &AppState) -> adw::PreferencesGroup {
    let rotation_group = adw::PreferencesGroup::builder().title("Rotation").build();
    let rotation = state.catalog.rotation().unwrap_or_default();
    let enabled = adw::SwitchRow::builder()
        .title("Automatic rotation")
        .subtitle("Advance during scheduled maintenance")
        .active(rotation.enabled)
        .build();
    enabled.connect_active_notify({
        let catalog = Rc::clone(&state.catalog);
        move |row| update_rotation(&catalog, |rotation| rotation.enabled = row.is_active())
    });
    rotation_group.add(&enabled);
    let random = adw::SwitchRow::builder()
        .title("Random order")
        .active(rotation.random)
        .build();
    random.connect_active_notify({
        let catalog = Rc::clone(&state.catalog);
        move |row| update_rotation(&catalog, |rotation| rotation.random = row.is_active())
    });
    rotation_group.add(&random);

    let playlists = state.playlists.borrow().clone();
    let mut names = vec!["All downloaded wallpapers".to_owned()];
    names.extend(playlists.iter().map(|playlist| playlist.name.clone()));
    let names = names.iter().map(String::as_str).collect::<Vec<_>>();
    let target = adw::ComboRow::builder()
        .title("Collection")
        .model(&gtk::StringList::new(&names))
        .selected(
            rotation
                .playlist_id
                .and_then(|id| playlists.iter().position(|playlist| playlist.id == id))
                .and_then(|index| u32::try_from(index + 1).ok())
                .unwrap_or_default(),
        )
        .build();
    target.connect_selected_notify({
        let catalog = Rc::clone(&state.catalog);
        move |row| {
            let playlist_id = row
                .selected()
                .checked_sub(1)
                .and_then(|index| playlists.get(index as usize))
                .map(|playlist| playlist.id);
            update_rotation(&catalog, |rotation| rotation.playlist_id = playlist_id);
        }
    });
    rotation_group.add(&target);

    let interval = spin_row(
        "Rotation interval (minutes)",
        5.0,
        1440.0,
        f64::from(rotation.interval_minutes),
    );
    interval.connect_value_notify({
        let catalog = Rc::clone(&state.catalog);
        move |row| {
            update_rotation(&catalog, |rotation| {
                rotation.interval_minutes = spin_value(row);
            });
        }
    });
    rotation_group.add(&interval);
    rotation_group
}

fn update_rotation(catalog: &Catalog, update: impl FnOnce(&mut RotationSettings)) {
    let mut rotation = catalog.rotation().unwrap_or_default();
    update(&mut rotation);
    catalog.set_rotation(&rotation).ok();
}

fn selected_item(state: &AppState) -> Option<CatalogItem> {
    state
        .selected
        .get()
        .and_then(|index| state.items.borrow().get(index).cloned())
}

fn selected_playlist(state: &AppState) -> Option<Playlist> {
    let selected = state.widgets.playlist_dropdown.selected();
    selected
        .checked_sub(1)
        .and_then(|index| state.playlists.borrow().get(index as usize).cloned())
}

fn set_busy(state: &AppState, busy: bool) {
    state.widgets.search.set_sensitive(!busy);
    state.widgets.spinner.set_visible(busy);
    if busy {
        state.widgets.spinner.start();
    } else {
        state.widgets.spinner.stop();
    }
}

fn toast(state: &AppState, message: &str) {
    state
        .widgets
        .toast_overlay
        .add_toast(adw::Toast::new(message));
}

fn metadata_label() -> gtk::Label {
    gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build()
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .build();
    button.add_css_class("flat");
    button
}

fn text_button(icon: &str, label: &str) -> gtk::Button {
    gtk::Button::builder().icon_name(icon).label(label).build()
}

fn spin_row(title: &str, min: f64, max: f64, value: f64) -> adw::SpinRow {
    let row = adw::SpinRow::with_range(min, max, 1.0);
    row.set_title(title);
    row.set_value(value);
    row
}

fn spin_value(row: &adw::SpinRow) -> u32 {
    row.value().to_u32().unwrap_or_default()
}

fn install_css() {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
