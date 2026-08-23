use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::Cell;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;

const APP_ID: &str = "com.github.cosmicfusion.nobara-kernel-manager";
const APP_ICON: &str = "com.github.cosmicfusion.nobara-kernel-manager";
const APP_NAME: &str = "Nobara Kernel Manager";

#[derive(Clone, Debug, PartialEq, Eq)]
struct BootEntry {
    title: String,
    version: String,
    linux: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum KernelState {
    Mainline,
    Lts,
    ThirdParty(Vec<String>),
    Mixed,
    Unknown(String),
}

#[derive(Clone)]
struct Controls {
    mainline: gtk::Button,
    lts: gtk::Button,
    rescue: gtk::Button,
    status: gtk::Label,
    detail: gtk::Label,
    spinner: gtk::Spinner,
    busy: Rc<Cell<bool>>,
}

enum BackendEvent {
    Line(String),
    Finished(bool),
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        gio::resources_register_include!("data.gresource")
            .expect("failed to register application resources");
        let display = gtk::gdk::Display::default().expect("could not connect to a display");
        gtk::IconTheme::for_display(&display).add_resource_path(
            "/com/github/cosmicfusion/nobara-kernel-manager/icons/scalable/actions/",
        );
    });
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    glib::set_application_name(APP_NAME);
    glib::set_prgname(Some("nobara-kernel-manager"));

    let header = adw::HeaderBar::builder()
        .title_widget(&adw::WindowTitle::builder().title(APP_NAME).build())
        .build();
    let icon = gtk::Image::builder()
        .icon_name("tux-symbolic")
        .pixel_size(96)
        .margin_top(24)
        .margin_bottom(12)
        .build();
    icon.add_css_class("accent");
    let heading = gtk::Label::builder()
        .label("Choose your Nobara kernel")
        .css_classes(["title-1"])
        .build();
    let description = gtk::Label::builder()
        .label("Switch the installed kernel family or rebuild the emergency rescue entry.")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .margin_bottom(18)
        .build();
    let status = gtk::Label::builder()
        .label("Checking installed kernels…")
        .css_classes(["heading"])
        .halign(gtk::Align::Center)
        .build();
    let detail = gtk::Label::builder()
        .wrap(true)
        .justify(gtk::Justification::Center)
        .halign(gtk::Align::Center)
        .margin_start(24)
        .margin_end(24)
        .margin_bottom(12)
        .build();
    let spinner = gtk::Spinner::builder()
        .spinning(true)
        .halign(gtk::Align::Center)
        .margin_bottom(12)
        .build();

    let mainline_button = gtk::Button::builder()
        .label("Switch to Mainline")
        .valign(gtk::Align::Center)
        .build();
    mainline_button.add_css_class("suggested-action");
    let mainline_row = adw::ActionRow::builder()
        .title("Mainline Kernel")
        .subtitle("The newest kernel offered by Nobara")
        .activatable_widget(&mainline_button)
        .build();
    mainline_row.add_suffix(&mainline_button);

    let lts_button = gtk::Button::builder()
        .label("Switch to LTS")
        .valign(gtk::Align::Center)
        .build();
    lts_button.add_css_class("suggested-action");
    let lts_row = adw::ActionRow::builder()
        .title("LTS Kernel")
        .subtitle("Nobara's long-term-support kernel")
        .activatable_widget(&lts_button)
        .build();
    lts_row.add_suffix(&lts_button);

    let rescue_button = gtk::Button::builder()
        .label("Reinstall Rescue Kernel")
        .valign(gtk::Align::Center)
        .build();
    let rescue_row = adw::ActionRow::builder()
        .title("Rescue Kernel")
        .subtitle("Rebuild a minimal, basic-graphics emergency boot entry")
        .activatable_widget(&rescue_button)
        .build();
    rescue_row.add_suffix(&rescue_button);

    let kernel_group = adw::PreferencesGroup::builder()
        .title("Kernel selection")
        .description("Switching removes prior managed kernel versions and installs the latest selected kernel.")
        .build();
    kernel_group.add(&mainline_row);
    kernel_group.add(&lts_row);
    let rescue_group = adw::PreferencesGroup::builder()
        .title("Recovery")
        .description(
            "The rescue action remains available even when a third-party kernel is detected.",
        )
        .build();
    rescue_group.add(&rescue_row);

    let groups = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_start(24)
        .margin_end(24)
        .margin_bottom(24)
        .build();
    groups.append(&kernel_group);
    groups.append(&rescue_group);
    let page = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    page.append(&icon);
    page.append(&heading);
    page.append(&description);
    page.append(&status);
    page.append(&detail);
    page.append(&spinner);
    page.append(&groups);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&page)
        .build();
    let toolbar = adw::ToolbarView::builder().content(&scroller).build();
    toolbar.add_top_bar(&header);
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(APP_NAME)
        .icon_name(APP_ICON)
        .default_width(640)
        .default_height(590)
        .content(&toolbar)
        .build();

    let controls = Controls {
        mainline: mainline_button,
        lts: lts_button,
        rescue: rescue_button,
        status,
        detail,
        spinner,
        busy: Rc::new(Cell::new(false)),
    };
    controls.mainline.connect_clicked(glib::clone!(
        #[weak]
        window,
        #[strong]
        controls,
        move |_| run_action(&window, &controls, "switch", "mainline")
    ));
    controls.lts.connect_clicked(glib::clone!(
        #[weak]
        window,
        #[strong]
        controls,
        move |_| run_action(&window, &controls, "switch", "lts")
    ));
    controls.rescue.connect_clicked(glib::clone!(
        #[weak]
        window,
        #[strong]
        controls,
        move |_| run_action(&window, &controls, "rescue", "reinstall")
    ));
    refresh_state(&controls);
    window.present();
}

fn run_action(
    window: &adw::ApplicationWindow,
    controls: &Controls,
    action: &'static str,
    option: &'static str,
) {
    set_controls_busy(controls, true);
    let log_buffer = gtk::TextBuffer::new(None);
    log_buffer.set_text("Starting privileged kernel operation…\n");
    let log_view = gtk::TextView::builder()
        .buffer(&log_buffer)
        .editable(false)
        .monospace(true)
        .cursor_visible(false)
        .left_margin(8)
        .right_margin(8)
        .top_margin(8)
        .bottom_margin(8)
        .build();
    let log_scroll = gtk::ScrolledWindow::builder()
        .child(&log_view)
        .width_request(560)
        .height_request(280)
        .build();
    let progress = gtk::Spinner::builder()
        .spinning(true)
        .halign(gtk::Align::Center)
        .margin_bottom(8)
        .build();
    let dialog_content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    dialog_content.append(&progress);
    dialog_content.append(&log_scroll);
    let heading = if action == "switch" {
        format!("Switching to {option}")
    } else {
        "Reinstalling rescue kernel".to_string()
    };
    let dialog = adw::AlertDialog::builder()
        .heading(&heading)
        .body("Do not power off the computer while kernel files are being changed.")
        .extra_child(&dialog_content)
        .build();
    dialog.add_response("close", "Close");
    dialog.set_response_enabled("close", false);
    dialog.present(Some(window));

    let (sender, receiver) = async_channel::unbounded();
    std::thread::spawn(move || {
        let mut child = match Command::new("pkexec")
            .arg(installed_or_source_script("kernel-manager"))
            .args([action, option])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                let _ = sender.send_blocking(BackendEvent::Line(format!(
                    "Failed to start kernel-manager: {error}"
                )));
                let _ = sender.send_blocking(BackendEvent::Finished(false));
                return;
            }
        };

        let stdout = child.stdout.take().expect("piped stdout was unavailable");
        let stderr = child.stderr.take().expect("piped stderr was unavailable");
        let stdout_sender = sender.clone();
        let stdout_reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.unwrap_or_else(|error| format!("Could not read output: {error}"));
                if stdout_sender
                    .send_blocking(BackendEvent::Line(line))
                    .is_err()
                {
                    break;
                }
            }
        });
        let stderr_sender = sender.clone();
        let stderr_reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let line = line.unwrap_or_else(|error| format!("Could not read output: {error}"));
                if stderr_sender
                    .send_blocking(BackendEvent::Line(line))
                    .is_err()
                {
                    break;
                }
            }
        });

        let status = child.wait();
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        let success = match status {
            Ok(status) => status.success(),
            Err(error) => {
                let _ = sender.send_blocking(BackendEvent::Line(format!(
                    "Could not wait for kernel-manager: {error}"
                )));
                false
            }
        };
        let _ = sender.send_blocking(BackendEvent::Finished(success));
    });
    glib::MainContext::default().spawn_local(glib::clone!(
        #[weak]
        dialog,
        #[weak]
        log_buffer,
        #[weak]
        log_view,
        #[weak]
        progress,
        #[strong]
        controls,
        async move {
            let mut received_output = false;
            while let Ok(event) = receiver.recv().await {
                match event {
                    BackendEvent::Line(line) => {
                        if !received_output {
                            log_buffer.set_text("");
                            dialog.set_body("Kernel operation in progress.");
                            received_output = true;
                        }
                        let mut end = log_buffer.end_iter();
                        log_buffer.insert(&mut end, &format!("{line}\n"));
                        log_view.scroll_to_iter(&mut end, 0.0, false, 0.0, 1.0);
                    }
                    BackendEvent::Finished(success) => {
                        progress.set_spinning(false);
                        dialog.set_response_enabled("close", true);
                        dialog.set_body(if success {
                            "The operation completed successfully."
                        } else {
                            "The operation failed. Review the output below."
                        });
                        set_controls_busy(&controls, false);
                        refresh_state(&controls);
                        break;
                    }
                }
            }
        }
    ));
}

fn set_controls_busy(controls: &Controls, busy: bool) {
    controls.busy.set(busy);
    if busy {
        controls.mainline.set_sensitive(false);
        controls.lts.set_sensitive(false);
        controls.rescue.set_sensitive(false);
        controls.status.set_label("Kernel operation in progress…");
        controls.detail.set_label("");
        controls.spinner.set_spinning(true);
        controls.spinner.set_visible(true);
    } else {
        controls.rescue.set_sensitive(true);
    }
}

fn refresh_state(controls: &Controls) {
    controls.mainline.set_sensitive(false);
    controls.lts.set_sensitive(false);
    controls.rescue.set_sensitive(true);
    controls.status.set_label("Checking installed kernels…");
    controls.detail.set_label("");
    controls.spinner.set_spinning(true);
    controls.spinner.set_visible(true);
    let (sender, receiver) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let _ = sender.send_blocking(detect_kernel_state());
    });
    glib::MainContext::default().spawn_local(glib::clone!(
        #[strong]
        controls,
        async move {
            if let Ok(state) = receiver.recv().await {
                if !controls.busy.get() {
                    apply_kernel_state(&controls, state);
                }
            }
        }
    ));
}

fn apply_kernel_state(controls: &Controls, state: KernelState) {
    controls.spinner.set_spinning(false);
    controls.spinner.set_visible(false);
    controls.rescue.set_sensitive(true);
    match state {
        KernelState::Mainline => {
            controls.status.set_label("Mainline kernel installed");
            controls
                .detail
                .set_label("Mainline is active; LTS is available as a replacement.");
            controls.mainline.set_sensitive(false);
            controls.mainline.set_label("Mainline Installed");
            controls.lts.set_sensitive(true);
            controls.lts.set_label("Switch to LTS");
        }
        KernelState::Lts => {
            controls.status.set_label("LTS kernel installed");
            controls
                .detail
                .set_label("LTS is active; Mainline is available as a replacement.");
            controls.mainline.set_sensitive(true);
            controls.mainline.set_label("Switch to Mainline");
            controls.lts.set_sensitive(false);
            controls.lts.set_label("LTS Installed");
        }
        KernelState::ThirdParty(kernels) => {
            controls.status.set_label("Third-party kernel detected");
            controls.detail.set_label(&format!(
                "3rd party kernel installed. Cannot install LTS or Mainline kernels.\n{}",
                kernels.join(", ")
            ));
            disable_switch_buttons(controls);
        }
        KernelState::Mixed => {
            controls
                .status
                .set_label("Multiple Nobara kernel families detected");
            controls.detail.set_label(
                "Both Mainline and LTS GRUB entries are installed. Kernel switching is disabled until only one family remains.",
            );
            disable_switch_buttons(controls);
        }
        KernelState::Unknown(error) => {
            controls
                .status
                .set_label("Unable to determine installed kernel");
            controls.detail.set_label(&error);
            disable_switch_buttons(controls);
        }
    }
}

fn disable_switch_buttons(controls: &Controls) {
    controls.mainline.set_sensitive(false);
    controls.lts.set_sensitive(false);
    controls.mainline.set_label("Unavailable");
    controls.lts.set_label("Unavailable");
}

fn detect_kernel_state() -> KernelState {
    match read_boot_entries() {
        Ok(entries) => classify_entries(&entries),
        Err(error) => KernelState::Unknown(error),
    }
}

fn read_boot_entries() -> Result<Vec<BootEntry>, String> {
    match read_boot_entries_from_dir(Path::new("/boot/loader/entries")) {
        Ok(entries) if !entries.is_empty() => Ok(entries),
        _ => read_boot_entries_with_helper(),
    }
}

fn read_boot_entries_from_dir(directory: &Path) -> Result<Vec<BootEntry>, String> {
    let mut entries = Vec::new();
    let files = fs::read_dir(directory)
        .map_err(|error| format!("Could not read {}: {error}", directory.display()))?;
    for file in files {
        let path = file.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("conf") {
            continue;
        }
        let contents = fs::read_to_string(&path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        if let Some(entry) = parse_bls_entry(&contents) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn read_boot_entries_with_helper() -> Result<Vec<BootEntry>, String> {
    let output = Command::new("pkexec")
        .arg(installed_or_source_script("kernel-status"))
        .output()
        .map_err(|error| format!("Could not start GRUB status helper: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Could not inspect GRUB entries: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let entries = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_helper_line)
        .collect::<Vec<_>>();
    if entries.is_empty() {
        Err("No Linux GRUB entries were found.".to_string())
    } else {
        Ok(entries)
    }
}

fn parse_bls_entry(contents: &str) -> Option<BootEntry> {
    let value = |key: &str| {
        contents.lines().find_map(|line| {
            let mut fields = line.splitn(2, char::is_whitespace);
            match (fields.next(), fields.next()) {
                (Some(found), Some(value)) if found == key => Some(value.trim().to_string()),
                _ => None,
            }
        })
    };
    Some(BootEntry {
        title: value("title")?,
        version: value("version")?,
        linux: value("linux")?,
    })
}

fn parse_helper_line(line: &str) -> Option<BootEntry> {
    let mut fields = line.splitn(3, '\t');
    Some(BootEntry {
        title: fields.next()?.to_string(),
        version: fields.next()?.to_string(),
        linux: fields.next()?.to_string(),
    })
}

fn classify_entries(entries: &[BootEntry]) -> KernelState {
    let mut has_mainline = false;
    let mut has_lts = false;
    let mut third_party = Vec::new();
    for entry in entries {
        let title = entry.title.to_lowercase();
        let version = entry.version.to_lowercase();
        if title.contains("rescue") || version.contains("rescue") {
            continue;
        }
        if !entry.linux.contains("vmlinuz") && !entry.linux.ends_with("/linux") {
            continue;
        }
        if version.contains(".lts.nobara.") {
            has_lts = true;
        } else if version.contains(".nobara.") {
            has_mainline = true;
        } else {
            third_party.push(format!("{} ({})", entry.title, entry.version));
        }
    }
    if !third_party.is_empty() {
        KernelState::ThirdParty(third_party)
    } else {
        match (has_mainline, has_lts) {
            (true, false) => KernelState::Mainline,
            (false, true) => KernelState::Lts,
            (true, true) => KernelState::Mixed,
            (false, false) => {
                KernelState::Unknown("No managed Nobara kernel GRUB entry was found.".to_string())
            }
        }
    }
}

fn installed_or_source_script(name: &str) -> PathBuf {
    let installed = PathBuf::from("/usr/lib/nobara-kernel-manager").join(name);
    if installed.exists() {
        installed
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("data/scripts")
            .join(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, version: &str) -> BootEntry {
        BootEntry {
            title: title.to_string(),
            version: version.to_string(),
            linux: format!("/vmlinuz-{version}"),
        }
    }

    #[test]
    fn detects_mainline_and_ignores_rescue() {
        let entries = vec![
            entry("Nobara Linux", "7.1.4-200.nobara.fc44.x86_64"),
            entry("Nobara Linux Rescue Kernel", "0-rescue-machine-id"),
        ];
        assert_eq!(classify_entries(&entries), KernelState::Mainline);
    }

    #[test]
    fn detects_lts() {
        let entries = vec![entry("Nobara Linux", "6.18.42-200.lts.nobara.fc44.x86_64")];
        assert_eq!(classify_entries(&entries), KernelState::Lts);
    }

    #[test]
    fn blocks_third_party_kernel() {
        let entries = vec![
            entry("Nobara Linux", "6.18.42-200.lts.nobara.fc44.x86_64"),
            entry("CachyOS Linux", "6.18.1-cachyos"),
        ];
        assert!(matches!(
            classify_entries(&entries),
            KernelState::ThirdParty(_)
        ));
    }

    #[test]
    fn parses_bls_fields() {
        let entry = parse_bls_entry(
            "title Nobara Linux\nversion 7.1.4-200.nobara.fc44.x86_64\nlinux /vmlinuz-test\n",
        )
        .unwrap();
        assert_eq!(entry.title, "Nobara Linux");
        assert_eq!(entry.linux, "/vmlinuz-test");
    }
}
