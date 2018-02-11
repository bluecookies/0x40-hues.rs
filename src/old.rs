extern crate ggez;
extern crate zip;
extern crate image;

use std::fs::File;
use std::path::PathBuf;
use std::io::Read;
use std::ffi::OsStr;


use std::thread;
use std::sync::mpsc::{channel, Receiver};

use ggez::event;
use ggez::{Context, ContextBuilder, GameResult};
use ggez::conf::{WindowSetup, WindowMode};
use ggez::graphics;
use ggez::graphics::{Image, DrawParam};

use zip::read::ZipArchive;

use image::ImageFormat;

use std::collections::HashMap;
use ggez::audio::{SoundData, Source};

// First we make a structure to contain the game's state
struct MainState {
	font: graphics::Font,
	load_text: graphics::Text,
	frames: usize,

	playing: Option<Source>,
	audio: HashMap<String, SoundData>,
	images: Vec<Image>,

	loaded: bool,
	//=================
	rx: Receiver<LoadStatus>,
	total_size: u64,
	loaded_size: u64,
	last_loaded_size: u64,
}

enum LoadStatus {
	TotalSize(u64),
	LoadSize(u64),
	Done(ResPack)
}

struct ResPack {
	images: Vec<image::RgbaImage>,
	audio: HashMap<String, SoundData>
}

impl MainState {
	fn new(ctx: &mut Context) -> GameResult<MainState> {
		let font = graphics::Font::new(ctx, "/Test.ttf", 24)?;
		let text = graphics::Text::new(ctx, "Loading...", &font)?;

		graphics::set_background_color(ctx, graphics::WHITE);
		graphics::set_color(ctx, graphics::BLACK).unwrap();

		// Load resources
		// Multiple producer single consumer - load up a thread for each zip pack
		// Pass in a clone of tx
		let (tx, rx) = channel();
		thread::spawn(move || {
			let f = File::open("respacks/Temp.zip").unwrap();
			let total_size = f.metadata().unwrap().len();
			tx.send(LoadStatus::TotalSize(total_size)).unwrap();

			let mut archive = ZipArchive::new(f).unwrap();

			let mut images: Vec<image::RgbaImage> = Vec::new();
			let mut audio: HashMap<String, SoundData> = HashMap::new();

			let mut loaded_size: u64 = 0;
			for i in 0..archive.len() {
				let mut file = archive.by_index(i).unwrap();
				let path: PathBuf = file.name().into();

				match path.extension().and_then(OsStr::to_str) {
					Some("png") => {
						// ZipFile doesn't impl Seek
						//let img = image::load(BufReader::new(file), ImageFormat::PNG).unwrap().to_rgba();
						let size = file.compressed_size();
						let mut buffer = Vec::with_capacity(file.size() as usize);
						file.read_to_end(&mut buffer).unwrap();

						let img = image::load_from_memory_with_format(&buffer[..], ImageFormat::PNG).unwrap().to_rgba();
						images.push(img);

						loaded_size += size;
						tx.send(LoadStatus::LoadSize(loaded_size)).unwrap();
					},
					Some("mp3") => {
						let size = file.compressed_size();
						// Uh... yeah look at this later
						audio.insert(path.file_stem().unwrap().to_str().unwrap().to_owned(), SoundData::from_read(&mut file).unwrap());
					
						loaded_size += size;
						tx.send(LoadStatus::LoadSize(loaded_size)).unwrap();
					},
					_ => println!("{:?}", path)
				}
			}

			tx.send(LoadStatus::Done(ResPack {
				images,
				audio
			})).unwrap();
		});

		let s = MainState {
			font,
			load_text: text,
			frames: 0,

			images: Vec::new(),
			audio: HashMap::new(),
			playing: None,

			loaded: false,
			rx,
			total_size: 0,
			loaded_size: 0,
			last_loaded_size: 0
		};
		Ok(s)
	}
}


impl event::EventHandler for MainState {
	fn update(&mut self, ctx: &mut Context) -> GameResult<()> {
		let val = self.rx.try_recv();
		match val {
			Ok(LoadStatus::TotalSize(size)) => {
				self.total_size += size;
			},
			Ok(LoadStatus::LoadSize(size)) => {
				self.last_loaded_size = self.loaded_size;
				self.loaded_size = size;
			},
			Ok(LoadStatus::Done(pack)) => {
				for img in pack.images.iter() {
					let (width, height) = img.dimensions();
					self.images.push(Image::from_rgba8(ctx, width as u16, height as u16, img).unwrap());
				}
				self.audio = pack.audio;

				self.loaded = true;
				let source = Source::from_data(ctx, self.audio.get("loop_MissYou").unwrap().clone())?;
				source.play()?;
				self.playing = Some(source);

			}
			Err(_) => {}
		}

		Ok(())
	}

	fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
		graphics::clear(ctx);

		if !self.loaded {
			if self.last_loaded_size != self.loaded_size {
				let load_text = format!("Loading {}/{}", self.loaded_size, self.total_size);
				self.load_text = graphics::Text::new(ctx, &load_text, &self.font)?;
				self.last_loaded_size = self.loaded_size;
			}

			// Drawables are drawn from their top-left corner
			let dest_point = graphics::Point2::new(0.0, 0.0);
			graphics::draw(ctx, &self.load_text, dest_point, 0.0)?;
		} else {
			let img = &self.images[18];
			let scale = img.height() as f32 / graphics::get_screen_coordinates(ctx).h as f32; 
			let scale = graphics::Point2::new(scale, scale);
			graphics::draw_ex(ctx, img, DrawParam { scale,..Default::default()})?;
		}

		graphics::present(ctx);
		self.frames += 1;
		if (self.frames % 100) == 0 {
			println!("FPS: {}", ggez::timer::get_fps(ctx));
		}
		Ok(())
	}
}

fn main() {
	let cb = ContextBuilder::new("hues", "ggez")
		.window_setup(WindowSetup { title: "0x40-hues.rs".to_owned(), ..Default::default() })
		.window_mode(WindowMode { width: 1280, height: 720, ..Default::default() })
		.add_resource_path(std::env::var("CARGO_MANIFEST_DIR").unwrap() + "/respacks");
	let mut ctx = cb.build().unwrap();

	let state = &mut MainState::new(&mut ctx).unwrap();
	if let Err(e) = event::run(&mut ctx, state) {
		println!("Error encountered: {}", e);
	} else {
		println!("Game exited cleanly.");
	}
}
