#[macro_use]
extern crate serde_derive;
extern crate glob;
extern crate rand;
extern crate rodio;
extern crate sdl2;
extern crate zip;
extern crate toml;

use std::thread;
use std::sync::mpsc::channel;
use std::fs::File;
use std::io::Read;

use std::time::{Duration, Instant};
use std::ffi::OsStr;

use sdl2::pixels::Color as Colour;
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::render::BlendMode;

use rodio::source::{Buffered, Source};

use glob::glob;

mod mp3;
mod loader;
mod ui;
mod surface;
mod images;
mod songs;
mod screen;

use loader::LoadStatus;
use ui::TextUi;
use ui::UiLayout;
use images::ImageManager;
use songs::SongManager;
use screen::Screen;

type Error = Box<std::error::Error>;
type Result<T> = std::result::Result<T, Error>;
type AudioData = Buffered<Box<Source<Item = i16> + Send>>;

fn main() {
	let sdl_context = sdl2::init().unwrap();
	let video_subsystem = sdl_context.video().unwrap();
	let _audio_subsystem = sdl_context.audio().unwrap();

	let window = video_subsystem
		.window("0x40-hues.rs", 1280, 720)
		.position_centered()
		.build()
		.unwrap();

	sdl2::hint::set("SDL_RENDER_VSYNC", "1");

	let mut canvas = window.into_canvas().build().unwrap();
	let texture_creator = canvas.texture_creator();

	// TTF - Handle error
	let ttf_context = sdl2::ttf::init().expect("Could not init ttf");

	// Image
	sdl2::image::init(sdl2::image::INIT_PNG).expect("Could not init png");

	// Font
	let font = ttf_context
		.load_font("respacks/PetMe64.ttf", 12)
		.expect("Could not load font");

	// Events
	let mut event_pump = sdl_context.event_pump().unwrap();

	canvas.set_draw_color(Colour::RGB(0xFF, 0xFF, 0xFF));
	canvas.set_blend_mode(BlendMode::Blend);
	canvas.clear();
	canvas.present();

	let mut image_manager = ImageManager::new(&texture_creator);
	let mut song_manager = SongManager::new();

	// Config
	let config: Option<Config> = File::open("config.toml").map_err(Error::from).and_then(|mut config_file| {
		let mut config_string = String::new();
		config_file.read_to_string(&mut config_string)?;
		Ok(config_string)
	}).and_then(|cfg| toml::from_str::<Config>(&cfg).map_err(|err| format!("	{}", err).into())).ok();

	// Load resources
	// this feels kinda hacky - maybe I rewrite later
	let respacks = if let Some(Config { respacks: Some(ref packs), .. }) = config {
		packs.clone()
	} else {
		glob("respacks/*.zip")
			.expect("Could not do this")
			.filter_map(std::result::Result::ok)
			.filter_map(|path| path.file_stem().and_then(OsStr::to_str).map(str::to_owned))	// and_then is a flatmap
			.collect::<Vec<String>>()
	};

	let mut remaining_packs = respacks.len();

	let (tx, rx) = channel();
	for packname in respacks.iter() {
		let tx = tx.clone();
		let path = format!("respacks/{}.zip", packname);
		thread::spawn(move || loader::load_respack(path, tx).map_err(|err| println!("Error loading pack: {}", err)));
		println!("Loading {}", packname);
	}

	// Draw loading screen
	let mut load_text = TextUi::create("Loading...", &font, &texture_creator).unwrap();
	load_text.centre(0, 0, 1280, 720);
	let (mut loaded_size, mut total_size): (u64, u64) = (0, 0);
	'loading: loop {
		for event in event_pump.poll_iter() {
			if let Event::Quit { .. } = event {
				return;
			}
		}
		// Update loading
		let val = rx.try_recv();
		let mut changed = false;
		match val {
			Ok(LoadStatus::TotalSize(size)) => {
				total_size += size;
				changed = true;
			}
			Ok(LoadStatus::LoadSize(size)) => {
				loaded_size += size;
				changed = true;
			}
			Ok(LoadStatus::Done(pack)) => {
				image_manager.extend(pack.images);
				song_manager.extend(pack.songs);

				remaining_packs -= 1;
				if remaining_packs == 0 {
					break 'loading;
				}
			}
			Err(_) => {}
		}

		// Rerender text if changed
		if changed {
			let text = format!("Loading {}/{}", loaded_size, total_size);
			load_text = TextUi::create(text, &font, &texture_creator).unwrap();
			load_text.centre(0, 0, 1280, 720);
		}

		// Render
		canvas.clear();

		load_text.draw(&mut canvas).unwrap();

		canvas.present();
	}

	//
	let mut screen = Screen::new(&texture_creator);
	screen.clear(&mut canvas);

	let mut frame_timer = Instant::now();
	let mut num_frames = 0;

	let mut basic_ui = ui::BasicUi::new(&font, &texture_creator);

	image_manager.random_image(&mut basic_ui);

	match config {
		Some(Config { song: Some(song), .. }) => song_manager.play_song(song, &mut basic_ui).ok(),
		_ => None
	}.unwrap_or_else(|| song_manager.play_random(&mut basic_ui));

	'running: loop {
		for event in event_pump.poll_iter() {
			match event {
				Event::Quit { .. } => {
					break 'running;
				}
				Event::KeyDown { scancode, .. } => match scancode {
					Some(Scancode::F) => image_manager.toggle_full_auto(&mut basic_ui),
					Some(Scancode::J) => song_manager.prev_song(&mut basic_ui),
					Some(Scancode::K) => song_manager.next_song(&mut basic_ui),
					Some(Scancode::N) => image_manager.prev_image(&mut basic_ui),
					Some(Scancode::M) => image_manager.next_image(&mut basic_ui),
					_ => {}
				},
				_ => {}
			}
		}

		song_manager.update_beat(&mut screen, &mut image_manager, &mut basic_ui);

		// Clear screen with colour
		screen.clear(&mut canvas);

		// Draw image
		image_manager.draw_image(&mut canvas, &mut basic_ui);

		// Text
		basic_ui.draw(&mut canvas).unwrap();

		// Overlay blackout
		screen.draw(&mut canvas); // maybe make screen draw the image and ui too
							// maybe make the screen hold the canvas

		canvas.present();

		// Track fps
		num_frames += 1;
		if num_frames == 200 {
			let duration = frame_timer.elapsed();
			// #frames per second = num_frames / duration as secs
			println!("FPS: {:.3}", num_frames as f64 / duration_to_secs(duration));

			frame_timer = Instant::now();
			num_frames = 0;
		}
	}
}

#[derive(Deserialize, Debug)]
struct Config {
	respacks: Option<Vec<String>>,
	song: Option<String>
}

fn _duration_to_millis(d: Duration) -> f64 {
	let secs: u64 = d.as_secs();
	let nano: u32 = d.subsec_nanos();

	(secs as f64) * 1000.0 + (nano as f64) * 1e-6
}

fn duration_to_secs(d: Duration) -> f64 {
	let secs: u64 = d.as_secs();
	let nano: u32 = d.subsec_nanos();

	(secs as f64) + (nano as f64) * 1e-9
}
