extern crate sdl2;
extern crate zip;
extern crate rodio;
extern crate xml;
extern crate rand;

use std::thread;
use std::sync::mpsc::{channel};

use std::time::{Instant, Duration};


use sdl2::pixels::{Color as Colour, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::event::Event;
use sdl2::render::{Texture, BlendMode, Canvas, RenderTarget};
use sdl2::rwops::RWops;
use sdl2::surface::Surface;

use sdl2::image::ImageRWops;
// bug: surface isn't threadsafe
//use sdl2::surface::Surface;


use rodio::source::{Source, Buffered, Zero as ZeroSource};
use rodio::Sink;

use rand::Rng;

mod mp3;
mod loader;
mod ui;

use loader::LoadStatus;
use ui::TextUi;
use ui::UiLayout;

type Result<T> = std::result::Result<T, Box<std::error::Error>>;
type AudioData = Buffered<Box<Source<Item = i16> + Send>>;

fn empty_audio() -> AudioData {
	let source = Box::new(ZeroSource::new(2, 44100)) as Box<Source<Item = i16> + Send>;
	source.buffered()
}


struct Blur {
	blur_type: BlurType,
	num: u8,
	dist: f64,
	init: Instant,
}

enum BlurType {
	Horizontal,
	Vertical,
	None
}

impl Blur {
	fn blur_x<T: UiLayout>(&mut self, ui: &mut T) {
		self.blur_type = BlurType::Horizontal;
		self.dist = 40.0;
		self.init = Instant::now();

		ui.update_x_blur(1.0);
		ui.update_y_blur(0.0);
	}

	fn blur_y<T: UiLayout>(&mut self, ui: &mut T) {
		self.blur_type = BlurType::Vertical;
		self.dist = 40.0;
		self.init = Instant::now();

		ui.update_x_blur(0.0);
		ui.update_y_blur(1.0);
	}

	fn factor(&self) -> f64 {
		// blur decay rate
		(-15.0 * duration_to_secs(self.init.elapsed())).exp()
	}

	fn new(num: u8) -> Self {
		Blur {
			blur_type: BlurType::None,
			num,
			dist: 20.0,
			init: Instant::now(),
		}
	}
}

fn main() {
	let sdl_context = sdl2::init().unwrap();
	let video_subsystem = sdl_context.video().unwrap();
	let _audio_subsystem = sdl_context.audio().unwrap();

	let window = video_subsystem.window("0x40-hues.rs", 1280, 720)
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
	let font = ttf_context.load_font("respacks/PetMe64.ttf", 12).expect("Could not load font");
	
	// Events
	let mut event_pump = sdl_context.event_pump().unwrap();

	sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "1");

	canvas.set_draw_color(Colour::RGB(0xFF, 0xFF, 0xFF));
	canvas.set_blend_mode(BlendMode::Blend);
	canvas.clear();
	canvas.present();

	// Random
	let mut rng = rand::thread_rng();
	let rng = &mut rng;

	let mut images: Vec<Image> = Vec::new();

	let mut songs: Vec<Song> = Vec::new();
	//let mut song_index: Option<usize> = None;
	let mut curr_song;	// = None;
	
	// Load resources
	let respacks = vec!["CharPackagev0.03", "Defaults_v5.0", "osuPack"];
	let mut remaining_packs = respacks.len();

	let (tx, rx) = channel();
	for packname in respacks.iter() {
		let tx = tx.clone();
		let path = format!("respacks/{}.zip", packname);
		thread::spawn(move || loader::load_respack(path, tx));
		println!("Loading {}", packname);
	}

	// Draw loading screen
	let mut load_text = TextUi::create("Loading...", &font, &texture_creator).unwrap();
	load_text.centre(0, 0, 1280, 720);
	let mut total_size: u64 = 0;
	let mut loaded_size: u64 = 0;
	let mut last_loaded_size: u64 = 0;
	'loading: loop {
		for event in event_pump.poll_iter() {
			if let Event::Quit { .. } = event {
				return;
			}
		}
		// Update loading
		let val = rx.try_recv();
		match val {
			Ok(LoadStatus::TotalSize(size)) => {
				total_size += size;
			},
			Ok(LoadStatus::LoadSize(size)) => {
				last_loaded_size = loaded_size;
				loaded_size += size;
			},
			Ok(LoadStatus::Done(pack)) => {
				images.extend(pack.images.into_iter().map(|image_loader| {
					let texture = {
						let rwops = RWops::from_bytes(&image_loader.data[..]).unwrap();
						let surface = rwops.load_png().unwrap();
						texture_creator.create_texture_from_surface(surface).unwrap()
					};

					Image::from_loader(image_loader, texture)
				}));

				songs.extend(pack.songs);
				

				println!("Done!");
				println!("Loaded {} songs.", songs.len());

				remaining_packs -= 1;
				if remaining_packs == 0 {
					break 'loading;
				}
			}
			Err(_) => {}
		}

		// Rerender text if changed
		if loaded_size != last_loaded_size {
			let text = format!("Loading {}/{}", loaded_size, total_size);
			load_text = TextUi::create(text, &font, &texture_creator).unwrap();
			load_text.centre(0, 0, 1280, 720);
			last_loaded_size = loaded_size;
		}

		// Render
		canvas.clear();

		load_text.draw(&mut canvas).unwrap();

		canvas.present();
	}

	if images.is_empty() {
		println!("Temp error - no images loaded.");
		return;
	}
	if songs.is_empty() {
		println!("Temp error - no songs loaded.");
		return;
	}

	let beat_time = Instant::now();
	let mut beat_index = BeatIndex::Buildup(100);	// Uh - ignore this - fix later
	let mut curr_colour = Colour::RGBA(0x00, 0x00, 0x00, 0xFF);

	let mut blur = Blur::new(7);
	let mut blackout_init = None;
	let mut blackout = {
		// Maybe make same size as screen
		let mut surface = Surface::new(1280, 720, PixelFormatEnum::RGBA8888).unwrap();
		surface.fill_rect(None, Colour::RGBA(0x00, 0x00, 0x00, 0xFF)).unwrap();
		texture_creator.create_texture_from_surface(surface).unwrap()
	};

	//let mut curr_image = rng.choose_mut(&mut images).unwrap();
	let mut curr_image_index = rng.gen_range(0, images.len());

	let mut frame_timer = Instant::now();
	let mut num_frames = 0;

	canvas.set_draw_color(curr_colour);

	let mut basic_ui = ui::BasicUi::new(&font, &texture_creator);
	basic_ui.update_image(&images[curr_image_index].name);


	// Temp
	let sink = Sink::new(&rodio::default_endpoint().unwrap());
	//"DJ Genericname - Dear you"
	//"Vexare - The Clockmaker"
	if let Some(index) = get_song_index(&songs, "DJ Genericname - Dear you") {
		songs[index].play(&sink);
		curr_song = Some(&songs[index]);
		basic_ui.update_song(&songs[index].title);
	} else {
		let index = rng.gen_range(0, songs.len());
		songs[index].play(&sink);
		curr_song = Some(&songs[index]);
		basic_ui.update_song(&songs[index].title);
	}

	'running: loop {
		for event in event_pump.poll_iter() {
			match event {
				Event::Quit{..} => {
					break 'running;
				},
				_ => {}
			}
		}
		if let Some(song) = curr_song {
			let new_index = song.get_beat_index(beat_time.elapsed());
			if new_index != beat_index {
				match song.get_beat(new_index) {
					b'.' => {},
					b'-' => {
						curr_colour = random_colour(rng, &mut basic_ui);
						//
						curr_image_index = rng.gen_range(0, images.len());
						basic_ui.update_image(&images[curr_image_index].name);
						//
						blackout_init = None;
					},
					b'o' => {
						curr_colour = random_colour(rng, &mut basic_ui);
						//
						curr_image_index = rng.gen_range(0, images.len());
						basic_ui.update_image(&images[curr_image_index].name);
						//
						blur.blur_x(&mut basic_ui);

						blackout_init = None;
					},
					b'x' => {
						curr_colour = random_colour(rng, &mut basic_ui);
						//
						curr_image_index = rng.gen_range(0, images.len());
						basic_ui.update_image(&images[curr_image_index].name);
						//
						blur.blur_y(&mut basic_ui);

						blackout_init = None;
					},
					b'O' => {
						blur.blur_x(&mut basic_ui);
						blackout_init = None;
					},
					b'X' => {
						blur.blur_y(&mut basic_ui);
						blackout_init = None;
					},
					b':' => {
						curr_colour = random_colour(rng, &mut basic_ui);
						blackout_init = None;
					},
					b'+' => {
						blackout_init = Some(Instant::now());
					},
					ch => println!("TODO: {}", ch)
				}
				beat_index = new_index;
			}

			// Update ui text
			{
				let time = duration_to_secs(beat_time.elapsed());
				let buildup_time = duration_to_secs(song.buildup_duration);
				let loop_time = duration_to_secs(song.loop_duration);
				
				let beat_time = ((time - buildup_time) % loop_time) * 1000.0;
				let beat_index = match beat_index {
					BeatIndex::Loop(idx) => idx as i32,
					BeatIndex::Buildup(idx) =>	idx as i32 - song.buildup_rhythm.len() as i32,
				};

				basic_ui.update_time(beat_time as i32);
				basic_ui.update_beat(beat_index);
			}
		}
		canvas.clear();

		// Draw image
		let curr_image = &mut images[curr_image_index];
		canvas.set_draw_color(curr_colour);
		curr_image.draw(&mut blur, &mut canvas, &mut basic_ui).unwrap();

		// Text
		basic_ui.draw(&mut canvas).unwrap();

		// Overlay blackout
		if let Some(start) = blackout_init {
			let fade = duration_to_secs(start.elapsed()) * 10.0;
			// Maybe set a flag to check before drawing image
			// TODO: ^ do that
			if fade >= 1.0 {
				canvas.set_draw_color(Colour::RGB(0x00, 0x00, 0x00));
				canvas.fill_rect(None).unwrap();
			} else {
				let alpha = (fade * 256.0) as u8;
				blackout.set_alpha_mod(alpha);
				canvas.copy(&blackout, None, None).unwrap();
			}
		}

		canvas.present();

		// Track fps
		num_frames += 1;
		if num_frames == 200 {
			let duration = frame_timer.elapsed();
			// #frames per second = num_frames / duration as secs
			println!("FPS: {}", num_frames as f64 / duration_to_secs(duration));

			frame_timer = Instant::now();
			num_frames = 0;
		}
	}
}

pub struct Song {
	name: String,
	title: String,
	source: Option<String>,
	rhythm: Vec<u8>,

	buildup: Option<String>,
	buildup_rhythm: Vec<u8>,

	// Length of beat
	loop_beat_length: Duration,
	buildup_beat_length: Duration,

	// Total length of loop/duration
	loop_duration: Duration,
	buildup_duration: Duration,

	loop_audio: AudioData,
	buildup_audio: Option<AudioData>,
}

struct Image {
	name: String,
	image: Texture,
	fullname: Option<String>,
	source: Option<String>,
	source_other: Option<String>,
}

impl Image {
	fn from_loader(loader: loader::ImageLoader, texture: Texture) -> Self {
		Image {
			name: loader.name,
			image: texture,
			fullname: loader.fullname,
			source: loader.source,
			source_other: loader.source_other
		}
	}

	// TODO: align
	fn draw<T: RenderTarget, S: UiLayout>(&mut self, blur: &mut Blur, canvas: &mut Canvas<T>, ui: &mut S) -> Result<()> {
		match blur.blur_type {
			BlurType::Horizontal => {
				self.image.set_alpha_mod(0xFF / blur.num);

				let factor = blur.factor();
				let dist = blur.dist * factor;

				for x in (0..blur.num).map(|i| 2.0 * i as f64/(blur.num as f64 - 1.0) - 1.0) {
					let rect = Rect::new((x * dist) as i32, 0, 1280, 720);
					canvas.copy(&self.image, None, Some(rect))?;
				}

				if dist < 1.0 {
					blur.blur_type = BlurType::None;
					ui.update_x_blur(0.0);
				} else {
					ui.update_x_blur(factor);
				}
			},
			BlurType::Vertical => {
				self.image.set_alpha_mod(0xFF / blur.num);

				let factor = blur.factor();
				let dist = blur.dist * factor;

				for y in (0..blur.num).map(|i| 2.0 * i as f64/(blur.num as f64 - 1.0) - 1.0) {
					let rect = Rect::new(0, (y * dist) as i32, 1280, 720);
					canvas.copy(&self.image, None, Some(rect))?;
				}

				if dist < 1.0 {
					blur.blur_type = BlurType::None;
					ui.update_y_blur(0.0);
				} else {
					ui.update_y_blur(factor);
				}
			},
			BlurType::None => {
				self.image.set_alpha_mod(0xD0);
				canvas.copy(&self.image, None, None)?;
			}
		}
		Ok(())
	}
}


#[derive(Debug, PartialEq, Copy, Clone)]
enum BeatIndex {
	Buildup(usize),
	Loop(usize)
}

impl std::fmt::Debug for Song {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "Song: {}", self.title)
	}
}

impl Song {
	fn get_beat_index(&self, beat_time: Duration) -> BeatIndex {
		if beat_time >= self.buildup_duration {
			let beat_time = beat_time - self.buildup_duration;
			BeatIndex::Loop((duration_to_secs(beat_time) / duration_to_secs(self.loop_beat_length)) as usize)
		} else {
			BeatIndex::Buildup((duration_to_secs(beat_time) / duration_to_secs(self.buildup_beat_length)) as usize)
		}
	}

	fn get_beat(&self, beat_index: BeatIndex) -> u8 {
		match beat_index {
			BeatIndex::Loop(index) => self.rhythm[index % self.rhythm.len()],
			BeatIndex::Buildup(index) => self.buildup_rhythm[index % self.buildup_rhythm.len()]
		}
	}

	fn play(&self, sink: &Sink) {
		if let Some(ref buildup) = self.buildup_audio {
			sink.append(buildup.clone());
		}

		sink.append(self.loop_audio.clone().repeat_infinite());
	}
}


fn get_song_index<T: AsRef<str>>(songs: &Vec<Song>, title: T) -> Option<usize> {
	songs.iter().position(|ref song| song.title == title.as_ref())
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

fn random_colour<T: Rng, S: UiLayout>(rng: &mut T, ui: &mut S) -> Colour {
	let idx = rng.gen_range(0x00, HUES.len());
	let (hue, name) = HUES[idx];


	//ui.update_colour_index(idx);
	//ui.update_colour_name(name);
	ui.update_colour(idx, name);


	hue
}

// 0x40 hues
static HUES: [(Colour, &str); 0x40] = [
	(Colour {r: 0x00, g: 0x00, b: 0x00, a: 0xFF}, "black"),
	(Colour {r: 0x55, g: 0x00, b: 0x00, a: 0xFF}, "brick"),
	(Colour {r: 0xAA, g: 0x00, b: 0x00, a: 0xFF}, "crimson"),
	(Colour {r: 0xFF, g: 0x00, b: 0x00, a: 0xFF}, "red"),
	(Colour {r: 0x00, g: 0x55, b: 0x00, a: 0xFF}, "turtle"),
	(Colour {r: 0x55, g: 0x55, b: 0x00, a: 0xFF}, "sludge"),
	(Colour {r: 0xAA, g: 0x55, b: 0x00, a: 0xFF}, "brown"),
	(Colour {r: 0xFF, g: 0x55, b: 0x00, a: 0xFF}, "orange"),
	(Colour {r: 0x00, g: 0xAA, b: 0x00, a: 0xFF}, "green"),
	(Colour {r: 0x55, g: 0xAA, b: 0x00, a: 0xFF}, "grass"),
	(Colour {r: 0xAA, g: 0xAA, b: 0x00, a: 0xFF}, "maize"),
	(Colour {r: 0xFF, g: 0xAA, b: 0x00, a: 0xFF}, "citrus"),
	(Colour {r: 0x00, g: 0xFF, b: 0x00, a: 0xFF}, "lime"),
	(Colour {r: 0x55, g: 0xFF, b: 0x00, a: 0xFF}, "leaf"),
	(Colour {r: 0xAA, g: 0xFF, b: 0x00, a: 0xFF}, "chartreuse"),
	(Colour {r: 0xFF, g: 0xFF, b: 0x00, a: 0xFF}, "yellow"),
	(Colour {r: 0x00, g: 0x00, b: 0x55, a: 0xFF}, "midnight"),
	(Colour {r: 0x55, g: 0x00, b: 0x55, a: 0xFF}, "plum"),
	(Colour {r: 0xAA, g: 0x00, b: 0x55, a: 0xFF}, "pomegranate"),
	(Colour {r: 0xFF, g: 0x00, b: 0x55, a: 0xFF}, "rose"),
	(Colour {r: 0x00, g: 0x55, b: 0x55, a: 0xFF}, "swamp"),
	(Colour {r: 0x55, g: 0x55, b: 0x55, a: 0xFF}, "dust"),
	(Colour {r: 0xAA, g: 0x55, b: 0x55, a: 0xFF}, "dirt"),
	(Colour {r: 0xFF, g: 0x55, b: 0x55, a: 0xFF}, "blossom"),
	(Colour {r: 0x00, g: 0xAA, b: 0x55, a: 0xFF}, "sea"),
	(Colour {r: 0x55, g: 0xAA, b: 0x55, a: 0xFF}, "ill"),
	(Colour {r: 0xAA, g: 0xAA, b: 0x55, a: 0xFF}, "haze"),
	(Colour {r: 0xFF, g: 0xAA, b: 0x55, a: 0xFF}, "peach"),
	(Colour {r: 0x00, g: 0xFF, b: 0x55, a: 0xFF}, "spring"),
	(Colour {r: 0x55, g: 0xFF, b: 0x55, a: 0xFF}, "mantis"),
	(Colour {r: 0xAA, g: 0xFF, b: 0x55, a: 0xFF}, "brilliant"),
	(Colour {r: 0xFF, g: 0xFF, b: 0x55, a: 0xFF}, "canary"),
	(Colour {r: 0x00, g: 0x00, b: 0xAA, a: 0xFF}, "navy"),
	(Colour {r: 0x55, g: 0x00, b: 0xAA, a: 0xFF}, "grape"),
	(Colour {r: 0xAA, g: 0x00, b: 0xAA, a: 0xFF}, "mauve"),
	(Colour {r: 0xFF, g: 0x00, b: 0xAA, a: 0xFF}, "purple"),
	(Colour {r: 0x00, g: 0x55, b: 0xAA, a: 0xFF}, "cornflower"),
	(Colour {r: 0x55, g: 0x55, b: 0xAA, a: 0xFF}, "deep"),
	(Colour {r: 0xAA, g: 0x55, b: 0xAA, a: 0xFF}, "lilac"),
	(Colour {r: 0xFF, g: 0x55, b: 0xAA, a: 0xFF}, "lavender"),
	(Colour {r: 0x00, g: 0xAA, b: 0xAA, a: 0xFF}, "aqua"),
	(Colour {r: 0x55, g: 0xAA, b: 0xAA, a: 0xFF}, "steel"),
	(Colour {r: 0xAA, g: 0xAA, b: 0xAA, a: 0xFF}, "grey"),
	(Colour {r: 0xFF, g: 0xAA, b: 0xAA, a: 0xFF}, "pink"),
	(Colour {r: 0x00, g: 0xFF, b: 0xAA, a: 0xFF}, "bay"),
	(Colour {r: 0x55, g: 0xFF, b: 0xAA, a: 0xFF}, "marina"),
	(Colour {r: 0xAA, g: 0xFF, b: 0xAA, a: 0xFF}, "tornado"),
	(Colour {r: 0xFF, g: 0xFF, b: 0xAA, a: 0xFF}, "saltine"),
	(Colour {r: 0x00, g: 0x00, b: 0xFF, a: 0xFF}, "blue"),
	(Colour {r: 0x55, g: 0x00, b: 0xFF, a: 0xFF}, "twilight"),
	(Colour {r: 0xAA, g: 0x00, b: 0xFF, a: 0xFF}, "orchid"),
	(Colour {r: 0xFF, g: 0x00, b: 0xFF, a: 0xFF}, "magenta"),
	(Colour {r: 0x00, g: 0x55, b: 0xFF, a: 0xFF}, "azure"),
	(Colour {r: 0x55, g: 0x55, b: 0xFF, a: 0xFF}, "liberty"),
	(Colour {r: 0xAA, g: 0x55, b: 0xFF, a: 0xFF}, "royalty"),
	(Colour {r: 0xFF, g: 0x55, b: 0xFF, a: 0xFF}, "thistle"),
	(Colour {r: 0x00, g: 0xAA, b: 0xFF, a: 0xFF}, "ocean"),
	(Colour {r: 0x55, g: 0xAA, b: 0xFF, a: 0xFF}, "sky"),
	(Colour {r: 0xAA, g: 0xAA, b: 0xFF, a: 0xFF}, "periwinkle"),
	(Colour {r: 0xFF, g: 0xAA, b: 0xFF, a: 0xFF}, "carnation"),
	(Colour {r: 0x00, g: 0xFF, b: 0xFF, a: 0xFF}, "cyan"),
	(Colour {r: 0x55, g: 0xFF, b: 0xFF, a: 0xFF}, "turquoise"),
	(Colour {r: 0xAA, g: 0xFF, b: 0xFF, a: 0xFF}, "powder"),
	(Colour {r: 0xFF, g: 0xFF, b: 0xFF, a: 0xFF}, "white"),
];