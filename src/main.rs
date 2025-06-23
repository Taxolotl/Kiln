use eframe::egui::Context;
use eframe::{egui, Frame};

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
	let options = eframe::NativeOptions::default();

	eframe::run_native(
		"Kiln",
		options,
		Box::new(|_cc| Ok(Box::new(State::default()))),
	)
}

struct State {
	is_loading: bool,
	/*
	current_project: Option<Modpack>,
	project_path: Option<PathBuf>,
	status_log: Arc<Mutex<Vec<String>>>,
	backup_log: Vec<String>,

	add_mod_name: String,
	add_mod_use_modrinth: bool,
	add_mod_modrinth: String,
	add_mod_use_curseforge: bool,
	add_mod_curseforge: String,

	is_checking: bool,
	check_task: Option<JoinHandle<()>>,

	is_exporting: bool,
	export_task: Option<JoinHandle<()>>,
	export_format: ExportFormat,
	*/

	screen: Screen,
}

impl Default for State {
	fn default() -> Self {
		Self {
			is_loading: true,
			screen: Screen::default(),
		}
	}
}

#[derive(Default)]
enum Screen {
	#[default]
	Loading,
	Home,
	New,
}

impl State {
	fn draw_sidebar(&mut self, ui: &mut egui::Ui) {
		ui.horizontal(|ui| {
			if ui.button("🏠").on_hover_text("Home").clicked() {
				self.screen = Screen::Home;
			}
			if ui.button("+").on_hover_text("Add").clicked() {
				self.screen = Screen::New;
			}
		});
	}

	fn draw_home_screen(&mut self, ui: &mut egui::Ui) {
		self.draw_sidebar(ui);

		ui.heading("Home");
	}

	fn draw_new_screen(&mut self, ui: &mut egui::Ui) {
		self.draw_sidebar(ui);

		ui.heading("New");
	}

	fn draw_loading(&mut self, ui: &mut egui::Ui) {
		ui.heading("Loading");
		if !self.is_loading {
			self.screen = Screen::Home;
		} else {
			self.load();
		}
	}

	fn load(&mut self) {
		self.is_loading = false;
	}
}

impl eframe::App for State {
	fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
		egui::CentralPanel::default().show(ctx, |ui| {
			match self.screen {
				Screen::Loading => {
					self.draw_loading(ui);
				}
				Screen::Home => {
					self.draw_home_screen(ui);
				},
				Screen::New => {
					self.draw_new_screen(ui);
				}
			}
		});
	}
}