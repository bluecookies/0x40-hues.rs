extern crate xml;

use std;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use std::io::{BufReader, Cursor, Read};

use std::sync::mpsc::Sender;

use std::collections::HashMap;

use zip::read::{ZipArchive, ZipFile};
use loader::xml::reader::{EventReader, XmlEvent};
use rodio::source::Source;

use sdl2::rwops::RWops;
use sdl2::image::ImageRWops;
//use sdl2::surface::SurfaceContext;

use mp3::Mp3Decoder;
use songs::Song;
use surface::Surface;
use Result;

pub enum LoadStatus {
	TotalSize(u64),
	LoadSize(u64),
	Done(ResPack),
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

pub struct SongData {
	pub name: String,
	pub title: String,
	pub source: Option<String>,
	pub rhythm: Vec<u8>,

	pub buildup: Option<String>,
	pub buildup_rhythm: Vec<u8>,
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

	let mut song_data = Vec::new();
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
			}
			Some("mp3") => {
				let mut data = Vec::with_capacity(file.size() as usize);
				file.read_to_end(&mut data).unwrap();

				let decoder = Mp3Decoder::new(Cursor::new(data));
				let source = (Box::new(decoder) as Box<Source<Item = i16> + Send>).buffered();
				audio.insert(name.to_owned(), source);
			}
			Some("xml") => {
				parse_xml(file, &mut song_data, &mut image_data);
			}
			_ => println!("{:?}", path),
		}
		tx.send(LoadStatus::LoadSize(size)).unwrap();
	}

	// Process songs
	let songs: Vec<Song> = song_data
		.into_iter()
		.filter_map(|data| Song::new(data, &mut audio).ok())
		.collect();

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
		songs,
	})).unwrap();
}

//XML
enum State {
	Document,
	Songs,
	Song(Option<SongField>),
	Images,
	Image(Option<ImageField>),
}
#[derive(Copy, Clone, Debug)]
enum SongField {
	Title,
	Source,
	Rhythm,
	Buildup,
	BuildupRhythm,
}
#[derive(Copy, Clone, Debug)]
enum ImageField {
	Source,
	SourceOther,
	FullName,
	Align,
	FrameDuration, // TODO: handle animations
}

// based off code from stebalien on rust-lang
// ok this got ugly, clean it up
fn parse_xml(file: ZipFile, songs: &mut Vec<SongData>, images: &mut Vec<ImageData>) {
	let mut reader = EventReader::new(BufReader::new(file));

	let mut state = State::Document;

	let mut song_name = None;
	let mut song_title = None;
	let mut song_source = None;
	let mut song_rhythm = Vec::new();
	let mut song_buildup = None;
	let mut song_buildup_rhythm = Vec::new();

	let mut image_filename = None;
	let mut image_name = None;
	let mut image_source = None;
	let mut image_source_other = None;
	// TODO: handle smart align
	//let mut image_align = None;

	while let Ok(event) = reader.next() {
		state = match state {
			State::Document => match event {
				XmlEvent::StartDocument { .. } => State::Document,
				XmlEvent::StartElement { name, .. } => match name.local_name.as_ref() {
					"songs" => State::Songs,
					"images" => State::Images,
					_ => {
						println!("Unknown xml tag {}", name.local_name);
						xml_skip_tag(&mut reader).unwrap();
						State::Document
					}
				},
				XmlEvent::EndDocument => break,
				_ => panic!("Unexpected"),
			},
			State::Songs => match event {
				XmlEvent::StartElement {
					name, attributes, ..
				} => {
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
				}
				XmlEvent::EndElement { .. } => State::Document,
				XmlEvent::Whitespace(_) => State::Songs,
				_ => panic!("Expected a song tag - got {:?}", event),
			},
			State::Song(None) => match event {
				XmlEvent::StartElement { ref name, .. } => match name.local_name.as_ref() {
					"title" => State::Song(Some(SongField::Title)),
					"source" => State::Song(Some(SongField::Source)),
					"rhythm" => State::Song(Some(SongField::Rhythm)),
					"buildup" => State::Song(Some(SongField::Buildup)),
					"buildupRhythm" => State::Song(Some(SongField::BuildupRhythm)),
					_ => {
						println!("Unknown song field {}", name.local_name);
						xml_skip_tag(&mut reader).unwrap();
						State::Song(None)
					}
				},
				XmlEvent::EndElement { .. } => {
					if song_rhythm.is_empty() {
						panic!("Empty rhythm");
					}

					let song = SongData {
						name: song_name.take().unwrap(),
						title: song_title.take().unwrap(),
						source: song_source.take(),
						rhythm: std::mem::replace(&mut song_rhythm, Vec::new()),
						buildup: song_buildup.take(),
						buildup_rhythm: std::mem::replace(&mut song_buildup_rhythm, Vec::new()),
					};

					songs.push(song);
					State::Songs
				}
				_ => State::Song(None),
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
						}
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
				}
				XmlEvent::EndElement { .. } => State::Song(None),
				_ => panic!("Expected data for tag {:?}", field),
			},
			State::Images => match event {
				XmlEvent::StartElement {
					name, attributes, ..
				} => {
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
				}
				XmlEvent::EndElement { .. } => State::Document,
				XmlEvent::Whitespace(_) => State::Images,
				_ => panic!("Expected an image tag - got {:?}", event),
			},
			State::Image(None) => match event {
				XmlEvent::StartElement { ref name, .. } => match name.local_name.as_ref() {
					"source" => State::Image(Some(ImageField::Source)),
					"source_other" => State::Image(Some(ImageField::SourceOther)),
					"fullname" => State::Image(Some(ImageField::FullName)),
					"align" => State::Image(Some(ImageField::Align)),
					"frameDuration" => State::Image(Some(ImageField::FrameDuration)),
					_ => {
						println!("Unknown image field {}", name.local_name);
						xml_skip_tag(&mut reader).unwrap();
						State::Image(None)
					}
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
				}
				_ => State::Image(None),
			},
			State::Image(Some(field)) => match event {
				XmlEvent::Characters(data) => {
					match field {
						ImageField::Source => image_source = Some(data),
						ImageField::SourceOther => image_source_other = Some(data),
						ImageField::FullName => image_name = Some(data),
						ImageField::Align => {}
						ImageField::FrameDuration => {}
					}
					State::Image(Some(field))
				}
				XmlEvent::EndElement { .. } => State::Image(None),
				_ => panic!("Expected data for tag {:?}", field),
			},
		}
	}
}

fn xml_skip_tag<R: Read>(reader: &mut EventReader<R>) -> Result<()> {
	let mut depth = 1;
	while depth > 0 {
		match reader.next() {
			Ok(XmlEvent::StartElement { .. }) => depth += 1,
			Ok(XmlEvent::EndElement { .. }) => depth -= 1,
			Ok(_event) => {}
			_ => return Err("Unexpected event error".into()),
		}
	}
	Ok(())
}
