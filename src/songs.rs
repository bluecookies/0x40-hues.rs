use std::fmt;
use std::time::{Duration, Instant};
use std::collections::HashMap;

use rand::{thread_rng as rng, Rng};

use sdl2::video::WindowContext;

use rodio;
use rodio::{Endpoint, Sink, Source};

use rodio::source::Zero as ZeroSource;

use duration_to_secs;
use AudioData;
use Screen;
use ui::UiLayout;
use images::ImageManager;
use loader::SongData;

use Result;

// Make this internal, take a SongLoader from loader
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

fn empty_audio() -> AudioData {
	let source = Box::new(ZeroSource::new(2, 44100)) as Box<Source<Item = i16> + Send>;
	source.buffered()
}

pub struct SongManager {
	songs: Vec<Song>,
	curr_index: Option<usize>,

	music_track: Sink,
	beat_time: Instant,

	beat_index: Option<BeatIndex>,

	endpoint: Endpoint,
}

impl SongManager {
	pub fn new() -> Self {
		let endpoint = rodio::default_endpoint().unwrap();
		SongManager {
			songs: Vec::new(),
			curr_index: None,

			beat_time: Instant::now(),
			beat_index: None,

			music_track: Sink::new(&endpoint),
			endpoint,
		}
	}

	pub fn extend(&mut self, songs: Vec<Song>) {
		self.songs.extend(songs);
	}

	pub fn play_song<T: AsRef<str>, S: UiLayout>(&mut self, name: T, ui: &mut S) -> Result<()> {
		self.get_song_index(name)
			.map(|index| {
				self.beat_time = Instant::now();
				self.beat_index = None;

				self.music_track = self.songs[index].play(&self.endpoint, ui);

				self.curr_index = Some(index);
			})
			.ok_or_else(|| "No song.".into())
	}

	pub fn play_random<S: UiLayout>(&mut self, ui: &mut S) {
		if self.songs.is_empty() {
			return;
		}
		let index = rng().gen_range(0, self.songs.len());
		self.beat_time = Instant::now();
		self.beat_index = None;

		self.music_track = self.songs[index].play(&self.endpoint, ui);

		self.curr_index = Some(index);
	}

	pub fn prev_song<T: UiLayout>(&mut self, ui: &mut T) {
		if self.songs.is_empty() {
			return;
		}

		let length = self.songs.len();
		let index = self.curr_index
			.map_or(0, move |index| (index + length - 1) % length);

		self.music_track = self.songs[index].play(&self.endpoint, ui);

		self.beat_time = Instant::now();
		self.beat_index = None;

		self.curr_index = Some(index);
	}

	pub fn next_song<T: UiLayout>(&mut self, ui: &mut T) {
		if self.songs.is_empty() {
			return;
		}

		let length = self.songs.len();
		let index = self.curr_index.map_or(0, move |index| (index + 1) % length);

		self.music_track = self.songs[index].play(&self.endpoint, ui);

		self.beat_time = Instant::now();
		self.beat_index = None;

		self.curr_index = Some(index);
	}

	pub fn update_beat<S: UiLayout>(
		&mut self,
		screen: &mut Screen,
		image_manager: &mut ImageManager<WindowContext>,
		ui: &mut S,
	) {
		if let Some(index) = self.curr_index {
			let song = &self.songs[index];

			let new_index = song.get_beat_index(self.beat_time.elapsed());
			if self.beat_index != Some(new_index) {
				match song.get_beat(new_index) {
					b'.' => {}
					b'-' => {
						screen.random_colour(ui);
						image_manager.random_image(ui);

						screen.clear_blackout();
					}
					b'o' => {
						screen.random_colour(ui);
						image_manager.random_image(ui);

						image_manager.blur_x(ui);
						screen.clear_blackout();
					}
					b'x' => {
						screen.random_colour(ui);
						image_manager.random_image(ui);

						image_manager.blur_y(ui);
						screen.clear_blackout();
					}
					b'O' => {
						image_manager.blur_x(ui);
						screen.clear_blackout();
					}
					b'X' => {
						image_manager.blur_y(ui);
						screen.clear_blackout();
					}
					b':' => {
						screen.random_colour(ui);
						screen.clear_blackout();
					}
					b'+' => {
						// blur x?
						image_manager.blur_x(ui);
						screen.blackout();
					}
					b'|' => {
						screen.short_blackout();
						screen.random_colour(ui);

						// check this
						image_manager.random_image(ui);
					}
					b'*' => {
						image_manager.random_image(ui);

						screen.clear_blackout();
					}
					b'=' => {
						image_manager.random_image(ui);

						// colourfade
						println!("TODO: colour fade");
						screen.clear_blackout();
					}
					b'~' => {
						// colour fade
						println!("TODO: colour fade");
						screen.clear_blackout();
					}
					ch => println!("TODO: {}", ch),
				}
				self.beat_index = Some(new_index);
			}

			// Update ui text
			{
				let time = duration_to_secs(self.beat_time.elapsed());
				let buildup_time = duration_to_secs(song.buildup_duration);
				let loop_time = duration_to_secs(song.loop_duration);

				let beat_time = ((time - buildup_time) % loop_time) * 1000.0;
				let beat_index = match self.beat_index {
					Some(BeatIndex::Loop(idx)) => idx as i32,
					Some(BeatIndex::Buildup(idx)) => idx as i32 - song.buildup_rhythm.len() as i32,
					None => 0,
				};

				ui.update_time(beat_time as i32);
				ui.update_beat(beat_index);
			}
		}
	}

	fn get_song_index<T: AsRef<str>>(&self, title: T) -> Option<usize> {
		self.songs
			.iter()
			.position(|ref song| song.title == title.as_ref())
	}
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum BeatIndex {
	Buildup(usize),
	Loop(usize),
}

impl fmt::Debug for Song {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "Song: {}", self.title)
	}
}

impl Song {
	pub fn new(song_data: SongData, audio_data: &mut HashMap<String, AudioData>) -> Result<Self> {
		let mut song = Song {
			name: song_data.name,
			title: song_data.title,
			source: song_data.source,
			rhythm: song_data.rhythm,

			buildup: song_data.buildup,
			buildup_rhythm: song_data.buildup_rhythm,

			// Length of beat
			loop_beat_length: Duration::new(0, 0),
			buildup_beat_length: Duration::new(0, 0),

			// Total length of loop/duration
			loop_duration: Duration::new(0, 0),
			buildup_duration: Duration::new(0, 0),

			loop_audio: empty_audio(),
			buildup_audio: None,
		};

		// Calculate beat length/buildup duration + fill in blank buildups
		if let Some(source) = audio_data.remove(&song.name) {
			song.loop_duration = source.total_duration().unwrap();
			song.loop_beat_length = song.loop_duration / song.rhythm.len() as u32;

			song.loop_audio = source;
		} else {
			return Err(format!("Error: Could not find song {}", song.name).into());
		}

		if let Some(ref buildup) = song.buildup {
			if let Some(source) = audio_data.remove(buildup) {
				song.buildup_duration = source.total_duration().unwrap();
				if song.buildup_rhythm.is_empty() {
					song.buildup_rhythm.push(b'.');
				}
				song.buildup_beat_length = song.buildup_duration / song.buildup_rhythm.len() as u32;

				song.buildup_audio = Some(source);
			} else {
				return Err(format!("Error: Could not find song {}", buildup).into());
			}
		}

		Ok(song)
	}

	fn get_beat_index(&self, beat_time: Duration) -> BeatIndex {
		if beat_time >= self.buildup_duration {
			let beat_time = beat_time - self.buildup_duration;
			BeatIndex::Loop(
				(duration_to_secs(beat_time) / duration_to_secs(self.loop_beat_length)) as usize,
			)
		} else {
			BeatIndex::Buildup(
				(duration_to_secs(beat_time) / duration_to_secs(self.buildup_beat_length)) as usize,
			)
		}
	}

	fn get_beat(&self, beat_index: BeatIndex) -> u8 {
		match beat_index {
			BeatIndex::Loop(index) => self.rhythm[index % self.rhythm.len()],
			BeatIndex::Buildup(index) => self.buildup_rhythm[index % self.buildup_rhythm.len()],
		}
	}

	fn play<T: UiLayout>(&self, endpoint: &Endpoint, ui: &mut T) -> Sink {
		let sink = Sink::new(endpoint);
		if let Some(ref buildup) = self.buildup_audio {
			sink.append(buildup.clone());
		}

		sink.append(self.loop_audio.clone().repeat_infinite());

		ui.update_song(&self.title);

		sink
	}
}
