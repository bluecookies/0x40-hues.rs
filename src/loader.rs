use std;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use std::io::{Read, Cursor, BufReader};

use std::sync::mpsc::Sender;

use std::collections::HashMap;
use std::time::{Duration};

use zip::read::{ZipArchive, ZipFile};
use xml::reader::{EventReader, XmlEvent};
use rodio::source::{Source};


use sdl2::rwops::RWops;
use sdl2::image::ImageRWops;
//use sdl2::surface::SurfaceContext;

use mp3::Mp3Decoder;
use songs::Song;
use surface::Surface;

pub enum LoadStatus {
	TotalSize(u64),
	LoadSize(u64),
	Done(ResPack)
}

// SDL2-rust implementation of surface isn't threadsafe for some reason
pub struct ResPack {
	pub images: Vec<ImageLoader>,
	pub songs: Vec<Song>,
}

pub struct ImageLoader {
	//data: SurfaceContext
	pub name: String,
	pub fullname: Option<String>,
	pub data: Surface,
	pub source: Option<String>,
	pub source_other: Option<String>,
}

impl ImageLoader {
	fn new(name: &str, buffer: Surface) -> Self {
		ImageLoader {
			name: name.to_owned(),
			data: buffer,
			fullname: None,
			source: None,
			source_other: None,
		}
	}

	fn add_data(&mut self, data: ImageData) {
		self.fullname = data.fullname;
		self.source = data.source;
		self.source_other = data.source_other;
	}
}

struct ImageData {
	filename: String,
	fullname: Option<String>,
	source: Option<String>,
	source_other: Option<String>,
	// align
	// frameDuration
}


pub fn load_respack<T: AsRef<Path>>(path: T, tx: Sender<LoadStatus>) {
	let f = File::open(path.as_ref()).unwrap();
	let total_size = f.metadata().unwrap().len();
	tx.send(LoadStatus::TotalSize(total_size)).unwrap();

	let mut archive = ZipArchive::new(f).unwrap();

	let mut images: HashMap<String, ImageLoader> = HashMap::new();
	let mut audio: HashMap<String, _> = HashMap::new();

	let mut songs = Vec::new();
	let mut image_data = Vec::new();
	for i in 0..archive.len() {
		let mut file = archive.by_index(i).unwrap();
		let path: PathBuf = file.name().into();

		let size = file.compressed_size();
		let name = path.file_stem().unwrap().to_str().unwrap();
		match path.extension().and_then(OsStr::to_str) {
			Some("png") => {
				let surface = {
					let mut buffer = Vec::with_capacity(file.size() as usize);
					file.read_to_end(&mut buffer).unwrap();

					let rwops = RWops::from_bytes(&buffer[..]).unwrap();
					let surface = rwops.load_png().unwrap();
					
					Surface::from_surface(surface).unwrap()
				};

				let image = ImageLoader::new(name, surface);

				images.insert(name.to_owned(), image);
			},
			Some("mp3") => {
				let mut data = Vec::with_capacity(file.size() as usize);
				file.read_to_end(&mut data).unwrap();

				audio.insert(name.to_owned(), Mp3Decoder::new(Cursor::new(data)));
			},
			Some("xml") => {
				match name {
					"songs" => {
						songs = parse_song_xml(file);
					},
					"images" => {
						image_data = parse_image_xml(file);
					},
					_ => println!("{:?}.xml", name),
				}
			},
			_ => println!("{:?}", path)
		}
		tx.send(LoadStatus::LoadSize(size)).unwrap();
	}

	// Process songs
	// Calculate beat length/buildup duration + fill in blank buildups

	// TODO: drain filter out the bad ones
	// TODO: move to song loader - can scrap out name and buildup name in Song, scrap durations in loader
	for song in songs.iter_mut() {
		if let Some(decoder) = audio.remove(&song.name) {
			song.loop_duration = decoder.total_duration().unwrap();
			song.loop_beat_length = song.loop_duration/song.rhythm.len() as u32;

			let source = Box::new(decoder) as Box<Source<Item = i16> + Send>;
			song.loop_audio = source.buffered();
		} else {
			println!("Error: Could not find song {}", &song.name);
		}

		if let Some(ref buildup) = song.buildup {
			if let Some(decoder) = audio.remove(buildup) {
				song.buildup_duration = decoder.total_duration().unwrap();
				if song.buildup_rhythm.is_empty() {
					song.buildup_rhythm.push(b'.');
				}
				song.buildup_beat_length = song.buildup_duration/song.buildup_rhythm.len() as u32;

				let source = Box::new(decoder) as Box<Source<Item = i16> + Send>;
				song.buildup_audio = Some(source.buffered());
			} else {
				println!("Error: Could not find song {}", buildup);
			}
		}
	}
	if !audio.is_empty() {
		println!("Warning: Unused audio data {:?}", audio.keys());
	}

	// Process images
	for image in image_data.into_iter() {
		if let Some(loader) = images.get_mut(&image.filename) {
			loader.add_data(image);
		} else {
			println!("Warning: Could not find image {}", image.filename);
		}
	}

	tx.send(LoadStatus::Done(ResPack {
		images: images.into_iter().map(|(_k, v)| v).collect(),
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

						loop_duration: Duration::new(0, 0),
						buildup_duration: Duration::new(0, 0),

						loop_audio: ::empty_audio(),
						buildup_audio: None,
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

fn parse_image_xml(file: ZipFile) -> Vec<ImageData> {
	enum State {
		Start,
		Images,
		Image(Option<ImageField>),
		End,
	}
	#[derive(Copy, Clone, Debug)]
	enum ImageField {
		Source,
		SourceOther,
		FullName,
		Align,
		FrameDuration,	// TODO: handle animations
	}

	let mut images = Vec::new();

	let mut reader = EventReader::new(BufReader::new(file));

	let mut state = State::Start;

	let mut image_filename = None;
	let mut image_name = None;
	let mut image_source = None;
	let mut image_source_other = None;
	
	// TODO: handle smart align
	//let mut image_align = None;

	while let Ok(event) = reader.next() {
		state = match state {
			State::Start => match event {
				XmlEvent::StartDocument { .. } => State::Start,
				XmlEvent::StartElement { ref name, .. } if name.local_name == "images" => State::Images,
				_ => panic!("Expected images tag")
			},
			State::End => match event {
				XmlEvent::EndDocument => break,
				_ => panic!("Expected eof")
			},
			State::Images => match event {
				XmlEvent::StartElement { name, attributes, .. } => {
					if name.local_name != "image" {
						panic!("Expected an image tag - got {}", name.local_name);
					}

					for attr in attributes.into_iter() {
						if attr.name.local_name == "name" {
							image_filename = Some(attr.value);
							break;
						}
					}

					if image_filename.is_none() {
						panic!("Expected an image name");
					}

					State::Image(None)
				},
				XmlEvent::EndElement { .. } => State::End,
				XmlEvent::Whitespace(_) => State::Images,
				_ => panic!("Expected an image tag - got {:?}", event)
			},
			State::Image(None) => match event {
				XmlEvent::StartElement { ref name, .. } => match name.local_name.as_ref() {
					"source" => State::Image(Some(ImageField::Source)),
					"source_other" => State::Image(Some(ImageField::SourceOther)),
					"fullname" => State::Image(Some(ImageField::FullName)),
					"align" => State::Image(Some(ImageField::Align)),
					"frameDuration" => State::Image(Some(ImageField::FrameDuration)),
					_ => panic!("Unknown image field {}", name.local_name)
				},
				XmlEvent::EndElement { .. } => {
					let image = ImageData {
						filename: image_filename.take().unwrap(),
						fullname: image_name.take(),
						source: image_source.take(),
						source_other: image_source_other.take(),
					};

					images.push(image);
					State::Images
				},
				_ => State::Image(None)
			},
			State::Image(Some(field)) => match event {
				XmlEvent::Characters(data) => {
					match field {
						ImageField::Source => image_source = Some(data),
						ImageField::SourceOther => image_source_other = Some(data),
						ImageField::FullName => image_name = Some(data),
						ImageField::Align => {},
						ImageField::FrameDuration => {},
					}
					State::Image(Some(field))
				},
				XmlEvent::EndElement { .. } => State::Image(None),
				_ => panic!("Expected data for tag {:?}", field)
			}
		}
	}

	return images;
}