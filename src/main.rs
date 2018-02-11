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

use sdl2::pixels::{Color as Colour};
use sdl2::rect::Rect;
use sdl2::event::Event;
use sdl2::render::{Canvas, Texture, TextureCreator, TextureQuery, RenderTarget};
use sdl2::ttf::Font;
use sdl2::rwops::RWops;

use sdl2::image::ImageRWops;
// bug: surface isn't threadsafe
//use sdl2::surface::Surface;

use zip::read::{ZipArchive, ZipFile};

use rodio::source::{Source, Buffered};

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
	
	// Load resources
	let mut load_text = TextUi::create("Loading...", &font, &texture_creator).unwrap();
	let mut total_size: u64 = 0;
	let mut loaded_size: u64 = 0;
	let mut last_loaded_size: u64 = 0;

	let (tx, rx) = channel();
	thread::spawn(move || load_respack("respacks/Temp.zip", tx.clone()));

	canvas.set_draw_color(Colour::RGB(0xFF, 0xFF, 0xFF));
	canvas.clear();
	canvas.present();

	let mut rng = rand::thread_rng();
	let rng = &mut rng;

	let mut images: Vec<Texture> = Vec::new();
	let mut audio: HashMap<String, Buffered<Box<Source<Item = i16> + Send>>> = HashMap::new();


	let mut songs: Vec<Song> = Vec::new();
	//let mut song_index: Option<usize> = None;
	let mut curr_song = None;

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
				println!("Done!");

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
				
				done_loading = true;
			}
			Err(_) => {}
		}

		if loaded_size != last_loaded_size {
			let text = format!("Loading {}/{}", loaded_size, total_size);
			load_text = TextUi::create(text, &font, &texture_creator).unwrap();
			loaded_size = last_loaded_size;
		}

		// Render
		canvas.clear();

		load_text.draw(&mut canvas).unwrap();

		canvas.present();
	}

	if let Some(index) = get_song_index(&songs, "Nhato - Miss You") {
		play_loop(&songs[index], &audio);
		curr_song = Some(&songs[index]);
	}

	//let mut beat_index: i32 = 0;
	let beat_time = Instant::now();
	let mut curr_colour = random_colour(rng);
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
				match song.get_beat(beat_time.elapsed()) {
					b'.' => {},
					b'o' => {
						curr_colour = random_colour(rng);
					},	//blur_x, change colour
					b'x' => {
						curr_colour = random_colour(rng);
					},	//blur_y, change colour
					b'O' => {},	//blur_x only
					b'X' => {},	//blur_y only
					ch => println!("TODO: {}", ch)
				}
			}


			canvas.set_draw_color(curr_colour);
			canvas.clear();

			canvas.copy(&images[0], None, None).unwrap();

			canvas.present();
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
	source: String,
	rhythm: Vec<u8>,

	buildup: Option<String>,
	buildup_rhythm: Option<Vec<u8>>,

	loop_duration: Duration,
	buildup_duration: Duration,
}

impl Song {
	fn get_beat(&self, beat_time: Duration) -> u8 {
		let beat_index = (duration_to_secs(beat_time) / duration_to_secs(self.loop_duration)) as usize;

		self.rhythm[beat_index % self.rhythm.len()]
	}

	// fn play_loop
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
			song.loop_duration = decoder.total_duration().unwrap()/song.rhythm.len() as u32;
		} else {
			println!("Error: Could not find song {}", &song.name);
		}
		if let Some(ref buildup) = song.buildup {
			if let Some(ref decoder) = audio.get(buildup) {
				song.buildup_duration = decoder.total_duration().unwrap()/song.buildup_rhythm.as_ref().unwrap().len() as u32;
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
	let mut song_buildup_rhythm = None;

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
						source: song_source.take().unwrap(),
						rhythm: std::mem::replace(&mut song_rhythm, Vec::new()),
						buildup: song_buildup.take(),
						buildup_rhythm: song_buildup_rhythm.take(),

						loop_duration: Duration::new(0, 0),
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
							song_buildup_rhythm = Some(data.into_bytes());
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

fn play_loop(song: &Song, audio: &HashMap<String, Buffered<Box<Source<Item = i16> + Send>>>) {
	let loop_name = &song.name;

	let endpoint = rodio::default_endpoint().unwrap();
	rodio::play_raw(&endpoint, audio.get(loop_name).unwrap().clone().convert_samples().repeat_infinite());
}


fn _sduration_to_millis(d: Duration) -> f64 {
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
	HUES[rng.gen_range(0x00, 0x40)]
}

//
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