extern crate sdl2;
extern crate zip;
extern crate rodio;
extern crate xml;
extern crate rand;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::io::{Read, Cursor, BufReader};
use std::ffi::OsStr;

use std::thread;
use std::sync::mpsc::{channel, Sender};

use std::collections::HashMap;

use sdl2::pixels::{Color as Colour, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::event::Event;
use sdl2::render::{Canvas, Texture, TextureCreator, TextureQuery, RenderTarget, BlendMode};
use sdl2::ttf::Font;
use sdl2::rwops::RWops;
use sdl2::surface::Surface;

use sdl2::image::ImageRWops;
// bug: surface isn't threadsafe
//use sdl2::surface::Surface;

use zip::read::{ZipArchive, ZipFile};

use rodio::source::{Source, Buffered};
use rodio::Sink;

use xml::reader::{EventReader, XmlEvent};

use std::time::{Instant, Duration};

use rand::Rng;

mod mp3;

use mp3::Mp3Decoder;

type Result<T> = std::result::Result<T, Box<std::error::Error>>;

struct TextUi {
	texture: Texture,
	rect: Rect,
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
	fn blur_x(&mut self) {
		self.blur_type = BlurType::Horizontal;
		self.dist = 40.0;
		self.init = Instant::now();
	}

	fn blur_y(&mut self) {
		self.blur_type = BlurType::Vertical;
		self.dist = 40.0;
		self.init = Instant::now();
	}

	fn factor(&self) -> f64 {
		// blur decay rate
		(-10.0 * duration_to_secs(self.init.elapsed())).exp()
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

impl TextUi {
	fn create<T: AsRef<str>, Target>(text: T, font: &Font, texture_creator: &TextureCreator<Target>) -> Result<Self> {
		let surface = font.render(text.as_ref()).blended(Colour::RGBA(0, 0, 0, 255))?;
		let texture = texture_creator.create_texture_from_surface(&surface)?;
		let TextureQuery { width, height, .. } = texture.query();
		let rect = Rect::new(0, 0, width, height);

		Ok(TextUi {
			texture,
			rect
		})
	}

	fn draw<T: RenderTarget>(&self, canvas: &mut Canvas<T>) -> Result<()> {
		canvas.copy(&self.texture, None, Some(self.rect))?;
		Ok(())
	}
}

fn main() {
	let sdl_context = sdl2::init().unwrap();
	let video_subsystem = sdl_context.video().unwrap();
	let _audio_subsystem = sdl_context.audio().unwrap();

	sdl2::hint::set("SDL_RENDER_VSYNC", "1");

	let window = video_subsystem.window("0x40-hues.rs", 1280, 720)
		.position_centered()
		.build()
		.unwrap();
 
	let mut canvas = window.into_canvas().build().unwrap();
	let texture_creator = canvas.texture_creator();

	// TTF - Handle error
	let ttf_context = sdl2::ttf::init().expect("Could not init ttf");

	// Image
	sdl2::image::init(sdl2::image::INIT_PNG).expect("Could not init png");

	// Font
	let font = ttf_context.load_font("respacks/Test.ttf", 24).unwrap();
	
	// FPS manager

	// Load resources
	let mut load_text = TextUi::create("Loading...", &font, &texture_creator).unwrap();
	let mut total_size: u64 = 0;
	let mut loaded_size: u64 = 0;
	let mut last_loaded_size: u64 = 0;


	let respacks = vec!["CharPackagev0.03", "Defaults_v5.0", "osuPack"];

	let (tx, rx) = channel();
	for packname in respacks.iter() {
		let tx = tx.clone();
		let path = format!("respacks/{}.zip", packname);
		thread::spawn(move || load_respack(path, tx));
	}

	canvas.set_draw_color(Colour::RGB(0xFF, 0xFF, 0xFF));
	canvas.set_blend_mode(BlendMode::Blend);
	canvas.clear();
	canvas.present();

	let mut rng = rand::thread_rng();
	let rng = &mut rng;

	let mut images: Vec<Texture> = Vec::new();
	let mut audio: HashMap<String, Buffered<Box<Source<Item = i16> + Send>>> = HashMap::new();


	let mut songs: Vec<Song> = Vec::new();
	//let mut song_index: Option<usize> = None;
	let mut curr_song;	// = None;

	let mut remaining_packs = respacks.len();
	let mut done_loading = false;
	let mut event_pump = sdl_context.event_pump().unwrap();
	'loading: while !done_loading {
		for event in event_pump.poll_iter() {
			match event {
				Event::Quit{..} => {
					break 'loading;
				},
				_ => {}
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
				images.extend(pack.images.into_iter().map(|surf| {
					let rwops = RWops::from_bytes(&surf[..]).unwrap();
					let surface = rwops.load_png().unwrap();
					texture_creator.create_texture_from_surface(surface).unwrap()
				}));
				audio.extend(pack.audio.into_iter().map(|(key, decoder)| {
					let source = Box::new(decoder) as Box<Source<Item = i16> + Send>;
					(key, source.buffered())
				}));
				songs.extend(pack.songs);
				

				println!("Done!");
				println!("Loaded {} songs.", songs.len());

				remaining_packs -= 1;
				if remaining_packs == 0 {
					done_loading = true;
				}
			}
			Err(_) => {}
		}

		if loaded_size != last_loaded_size {
			let text = format!("Loading {}/{}", loaded_size, total_size);
			load_text = TextUi::create(text, &font, &texture_creator).unwrap();
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


	let sink = Sink::new(&rodio::default_endpoint().unwrap());
	if let Some(index) = get_song_index(&songs, "DJ Genericname - Dear you") {
		songs[index].play(&sink, &audio);
		curr_song = Some(&songs[index]);
	} else {
		let index = rng.gen_range(0, songs.len());
		songs[index].play(&sink, &audio);
		curr_song = Some(&songs[index]);
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
	if done_loading {
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
							curr_colour = random_colour(rng);
							curr_image_index = rng.gen_range(0, images.len());
							blackout_init = None;
						},
						b'o' => {
							curr_colour = random_colour(rng);
							curr_image_index = rng.gen_range(0, images.len());
							blur.blur_x();

							blackout_init = None;
						},
						b'x' => {
							curr_colour = random_colour(rng);
							curr_image_index = rng.gen_range(0, images.len());
							blur.blur_y();

							blackout_init = None;
						},
						b'O' => {
							blur.blur_x();
							blackout_init = None;
						},
						b'X' => {
							blur.blur_y();
							blackout_init = None;
						},
						b':' => {
							curr_colour = random_colour(rng);
							blackout_init = None;
						},
						b'+' => {
							blackout_init = Some(Instant::now());
						},
						ch => println!("TODO: {}", ch)
					}
					beat_index = new_index;
				}
			}
			canvas.clear();

			// Draw image
			let curr_image = &mut images[curr_image_index];
			canvas.set_draw_color(curr_colour);
			match blur.blur_type {
				BlurType::Horizontal => {
					curr_image.set_alpha_mod(0xFF / blur.num);

					let factor = blur.factor();
					let dist = blur.dist * factor;

					for x in (0..blur.num).map(|i| 2.0 * i as f64/(blur.num as f64 - 1.0) - 1.0) {
						let rect = Rect::new((x * dist) as i32, 0, 1280, 720);
						canvas.copy(curr_image, None, Some(rect)).unwrap();
					}

					if dist < 1.0 {
						blur.blur_type = BlurType::None;
					}
				},
				BlurType::Vertical => {
					curr_image.set_alpha_mod(0xFF / blur.num);

					let factor = blur.factor();
					let dist = blur.dist * factor;

					for y in (0..blur.num).map(|i| 2.0 * i as f64/(blur.num as f64 - 1.0) - 1.0) {
						let rect = Rect::new(0, (y * dist) as i32, 1280, 720);
						canvas.copy(curr_image, None, Some(rect)).unwrap();
					}

					if dist < 1.0 {
						blur.blur_type = BlurType::None;
					}
				},
				BlurType::None => {
					curr_image.set_alpha_mod(0xD0);
					canvas.copy(curr_image, None, None).unwrap();
				}
			}

			// Overlay blackout
			if let Some(start) = blackout_init {
				let fade = duration_to_secs(start.elapsed()) * 10.0;
				// Maybe set a flag to check before drawing image
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

			num_frames += 1;
			if num_frames == 100 {
				let duration = frame_timer.elapsed();
				// #frames per second = num_frames / duration as secs
				println!("FPS: {}", num_frames as f64 / duration_to_secs(duration));

				frame_timer = Instant::now();
				num_frames = 0;
			}
		}
	}
}

// SDL2-rust implementation of surface isn't threadsafe for some reason
struct ResPack {
	images: Vec<Vec<u8>>,
	audio: HashMap<String, Mp3Decoder<Cursor<Vec<u8>>>>,
	songs: Vec<Song>,
}

#[derive(Debug)]
struct Song {
	name: String,
	title: String,
	source: Option<String>,
	rhythm: Vec<u8>,

	buildup: Option<String>,
	buildup_rhythm: Vec<u8>,

	loop_beat_length: Duration,
	buildup_beat_length: Duration,

	buildup_duration: Duration,
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum BeatIndex {
	Buildup(usize),
	Loop(usize)
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

	fn play(&self, sink: &Sink, audio: &HashMap<String, Buffered<Box<Source<Item = i16> + Send>>>) {
		if let Some(ref buildup) = self.buildup {
			sink.append(audio.get(buildup).unwrap().clone());
		}


		let loop_name = &self.name;
		sink.append(audio.get(loop_name).unwrap().clone().repeat_infinite());
	}
}

enum LoadStatus {
	TotalSize(u64),
	LoadSize(u64),
	Done(ResPack)
}

fn load_respack<T: AsRef<Path>>(path: T, tx: Sender<LoadStatus>) {
	let f = File::open(path.as_ref()).unwrap();
	let total_size = f.metadata().unwrap().len();
	tx.send(LoadStatus::TotalSize(total_size)).unwrap();

	let mut archive = ZipArchive::new(f).unwrap();

	let mut images = Vec::new();
	let mut audio: HashMap<String, _> = HashMap::new();

	let mut songs = Vec::new();
	for i in 0..archive.len() {
		let mut file = archive.by_index(i).unwrap();
		let path: PathBuf = file.name().into();

		let size = file.compressed_size();
		match path.extension().and_then(OsStr::to_str) {
			Some("png") => {
				// ZipFile doesn't impl Seek
				let surface = {
					let mut buffer = Vec::with_capacity(file.size() as usize);
					file.read_to_end(&mut buffer).unwrap();

					//let rwops = RWops::from_bytes(&buffer[..]).unwrap();
					//let surface = rwops.load_png().unwrap();

					// temporary
					//let pixel_format = surface.pixel_format();
					//surface.convert(&pixel_format).unwrap()

					buffer
				};

				images.push(surface);
			},
			Some("mp3") => {
				let name = path.file_stem().unwrap().to_str().unwrap();

				let mut data = Vec::with_capacity(file.size() as usize);
				file.read_to_end(&mut data).unwrap();

				audio.insert(name.to_owned(), Mp3Decoder::new(Cursor::new(data)));
			},
			Some("xml") => {
				let name = path.file_stem().unwrap().to_str().unwrap();
				match name {
					"songs" => {
						songs = parse_song_xml(file);
					},
					_ => println!("{:?}.xml", name),
				}
			},
			_ => println!("{:?}", path)
		}
		tx.send(LoadStatus::LoadSize(size)).unwrap();
	}

	for song in songs.iter_mut() {
		if let Some(ref decoder) = audio.get(&song.name) {
			song.loop_beat_length = decoder.total_duration().unwrap()/song.rhythm.len() as u32;
		} else {
			println!("Error: Could not find song {}", &song.name);
		}
		if let Some(ref buildup) = song.buildup {
			if let Some(ref decoder) = audio.get(buildup) {
				song.buildup_duration = decoder.total_duration().unwrap();
				if song.buildup_rhythm.is_empty() {
					song.buildup_rhythm.push(b'.');
				}
				song.buildup_beat_length = song.buildup_duration/song.buildup_rhythm.len() as u32;
			} else {
				println!("Error: Could not find song {}", buildup);
			}
		}
	}



	tx.send(LoadStatus::Done(ResPack {
		images,
		audio,
		songs
	})).unwrap();
}

// based off code from stebalien on rust-lang
fn parse_song_xml(file: ZipFile) -> Vec<Song> {
	enum State {
		Start,
		Songs,
		Song(Option<SongField>),
		End,
	}
	#[derive(Copy, Clone, Debug)]
	enum SongField {
		Title,
		Source,
		Rhythm,
		Buildup,
		BuildupRhythm,
	}

	let mut songs = Vec::new();

	let mut reader = EventReader::new(BufReader::new(file));

	let mut state = State::Start;

	let mut song_name = None;
	let mut song_title = None;
	let mut song_source = None;
	let mut song_rhythm = Vec::new();
	let mut song_buildup = None;
	let mut song_buildup_rhythm = Vec::new();

	while let Ok(event) = reader.next() {
		state = match state {
			State::Start => match event {
				XmlEvent::StartDocument { .. } => State::Start,
				XmlEvent::StartElement { ref name, .. } if name.local_name == "songs" => State::Songs,
				_ => panic!("Expected songs tag")
			},
			State::End => match event {
				XmlEvent::EndDocument => break,
				_ => panic!("Expected eof")
			},
			State::Songs => match event {
				XmlEvent::StartElement { name, attributes, .. } => {
					if name.local_name != "song" {
						panic!("Expected a song tag - got {}", name.local_name);
					}

					for attr in attributes.into_iter() {
						if attr.name.local_name == "name" {
							song_name = Some(attr.value);
							break;
						}
					}

					if song_name.is_none() {
						panic!("Expected a song name");
					}

					State::Song(None)
				},
				XmlEvent::EndElement { .. } => State::End,
				XmlEvent::Whitespace(_) => State::Songs,
				_ => panic!("Expected a song tag - got {:?}", event)
			},
			State::Song(None) => match event {
				XmlEvent::StartElement { ref name, .. } => match name.local_name.as_ref() {
					"title" => State::Song(Some(SongField::Title)),
					"source" => State::Song(Some(SongField::Source)),
					"rhythm" => State::Song(Some(SongField::Rhythm)),
					"buildup" => State::Song(Some(SongField::Buildup)),
					"buildupRhythm" => State::Song(Some(SongField::BuildupRhythm)),
					_ => panic!("Unknown song field {}", name.local_name)
				},
				XmlEvent::EndElement { .. } => {
					if song_rhythm.is_empty() {
						panic!("Empty rhythm");
					}

					let song = Song {
						name: song_name.take().unwrap(),
						title: song_title.take().unwrap(),
						source: song_source.take(),
						rhythm: std::mem::replace(&mut song_rhythm, Vec::new()),
						buildup: song_buildup.take(),
						buildup_rhythm: std::mem::replace(&mut song_buildup_rhythm, Vec::new()),

						loop_beat_length: Duration::new(0, 0),
						buildup_beat_length: Duration::new(0, 0),

						buildup_duration: Duration::new(0, 0),
					};

					songs.push(song);
					State::Songs
				},
				_ => State::Song(None)
			},
			State::Song(Some(field)) => match event {
				XmlEvent::Characters(data) => {
					match field {
						SongField::Title => song_title = Some(data),
						SongField::Source => song_source = Some(data),
						SongField::Rhythm => {
							if !data.is_ascii() {
								panic!("Expected ascii characters in rhythm");
							}
							song_rhythm = data.into_bytes();
						},
						SongField::Buildup => song_buildup = Some(data),
						SongField::BuildupRhythm => {
							if !data.is_ascii() {
								panic!("Expected ascii characters in rhythm");
							}
							if data.is_empty() {
								panic!("Buildup rhythm empty!");
							}
							song_buildup_rhythm = data.into_bytes();
						}
					}
					State::Song(Some(field))
				},
				XmlEvent::EndElement { .. } => State::Song(None),
				_ => panic!("Expected data for tag {:?}", field)
			}
		}
	}

	return songs;
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

fn random_colour<T: Rng>(rng: &mut T) -> Colour {
	HUES[rng.gen_range(0x00, HUES.len())]
}

// 0x40 hues
static HUES: [Colour; 0x40] = [
	Colour {r: 0x00, g: 0x00, b: 0x00, a: 0xFF},
	Colour {r: 0x55, g: 0x00, b: 0x00, a: 0xFF},
	Colour {r: 0xAA, g: 0x00, b: 0x00, a: 0xFF},
	Colour {r: 0xFF, g: 0x00, b: 0x00, a: 0xFF},
	Colour {r: 0x00, g: 0x55, b: 0x00, a: 0xFF},
	Colour {r: 0x55, g: 0x55, b: 0x00, a: 0xFF},
	Colour {r: 0xAA, g: 0x55, b: 0x00, a: 0xFF},
	Colour {r: 0xFF, g: 0x55, b: 0x00, a: 0xFF},
	Colour {r: 0x00, g: 0xAA, b: 0x00, a: 0xFF},
	Colour {r: 0x55, g: 0xAA, b: 0x00, a: 0xFF},
	Colour {r: 0xAA, g: 0xAA, b: 0x00, a: 0xFF},
	Colour {r: 0xFF, g: 0xAA, b: 0x00, a: 0xFF},
	Colour {r: 0x00, g: 0xFF, b: 0x00, a: 0xFF},
	Colour {r: 0x55, g: 0xFF, b: 0x00, a: 0xFF},
	Colour {r: 0xAA, g: 0xFF, b: 0x00, a: 0xFF},
	Colour {r: 0xFF, g: 0xFF, b: 0x00, a: 0xFF},
	Colour {r: 0x00, g: 0x00, b: 0x55, a: 0xFF},
	Colour {r: 0x55, g: 0x00, b: 0x55, a: 0xFF},
	Colour {r: 0xAA, g: 0x00, b: 0x55, a: 0xFF},
	Colour {r: 0xFF, g: 0x00, b: 0x55, a: 0xFF},
	Colour {r: 0x00, g: 0x55, b: 0x55, a: 0xFF},
	Colour {r: 0x55, g: 0x55, b: 0x55, a: 0xFF},
	Colour {r: 0xAA, g: 0x55, b: 0x55, a: 0xFF},
	Colour {r: 0xFF, g: 0x55, b: 0x55, a: 0xFF},
	Colour {r: 0x00, g: 0xAA, b: 0x55, a: 0xFF},
	Colour {r: 0x55, g: 0xAA, b: 0x55, a: 0xFF},
	Colour {r: 0xAA, g: 0xAA, b: 0x55, a: 0xFF},
	Colour {r: 0xFF, g: 0xAA, b: 0x55, a: 0xFF},
	Colour {r: 0x00, g: 0xFF, b: 0x55, a: 0xFF},
	Colour {r: 0x55, g: 0xFF, b: 0x55, a: 0xFF},
	Colour {r: 0xAA, g: 0xFF, b: 0x55, a: 0xFF},
	Colour {r: 0xFF, g: 0xFF, b: 0x55, a: 0xFF},
	Colour {r: 0x00, g: 0x00, b: 0xAA, a: 0xFF},
	Colour {r: 0x55, g: 0x00, b: 0xAA, a: 0xFF},
	Colour {r: 0xAA, g: 0x00, b: 0xAA, a: 0xFF},
	Colour {r: 0xFF, g: 0x00, b: 0xAA, a: 0xFF},
	Colour {r: 0x00, g: 0x55, b: 0xAA, a: 0xFF},
	Colour {r: 0x55, g: 0x55, b: 0xAA, a: 0xFF},
	Colour {r: 0xAA, g: 0x55, b: 0xAA, a: 0xFF},
	Colour {r: 0xFF, g: 0x55, b: 0xAA, a: 0xFF},
	Colour {r: 0x00, g: 0xAA, b: 0xAA, a: 0xFF},
	Colour {r: 0x55, g: 0xAA, b: 0xAA, a: 0xFF},
	Colour {r: 0xAA, g: 0xAA, b: 0xAA, a: 0xFF},
	Colour {r: 0xFF, g: 0xAA, b: 0xAA, a: 0xFF},
	Colour {r: 0x00, g: 0xFF, b: 0xAA, a: 0xFF},
	Colour {r: 0x55, g: 0xFF, b: 0xAA, a: 0xFF},
	Colour {r: 0xAA, g: 0xFF, b: 0xAA, a: 0xFF},
	Colour {r: 0xFF, g: 0xFF, b: 0xAA, a: 0xFF},
	Colour {r: 0x00, g: 0x00, b: 0xFF, a: 0xFF},
	Colour {r: 0x55, g: 0x00, b: 0xFF, a: 0xFF},
	Colour {r: 0xAA, g: 0x00, b: 0xFF, a: 0xFF},
	Colour {r: 0xFF, g: 0x00, b: 0xFF, a: 0xFF},
	Colour {r: 0x00, g: 0x55, b: 0xFF, a: 0xFF},
	Colour {r: 0x55, g: 0x55, b: 0xFF, a: 0xFF},
	Colour {r: 0xAA, g: 0x55, b: 0xFF, a: 0xFF},
	Colour {r: 0xFF, g: 0x55, b: 0xFF, a: 0xFF},
	Colour {r: 0x00, g: 0xAA, b: 0xFF, a: 0xFF},
	Colour {r: 0x55, g: 0xAA, b: 0xFF, a: 0xFF},
	Colour {r: 0xAA, g: 0xAA, b: 0xFF, a: 0xFF},
	Colour {r: 0xFF, g: 0xAA, b: 0xFF, a: 0xFF},
	Colour {r: 0x00, g: 0xFF, b: 0xFF, a: 0xFF},
	Colour {r: 0x55, g: 0xFF, b: 0xFF, a: 0xFF},
	Colour {r: 0xAA, g: 0xFF, b: 0xFF, a: 0xFF},
	Colour {r: 0xFF, g: 0xFF, b: 0xFF, a: 0xFF},
];