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

pub struct Song {
	name: String,
	pub title: String,
	pub source: Option<String>,
	pub rhythm: Vec<char>,

	buildup: Option<String>,
	pub buildup_rhythm: Vec<char>,

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
					'.' => {}
					'-' => {
						screen.random_colour(ui);
						image_manager.random_image(ui);

						screen.clear_blackout();
					}
					'o' => {
						screen.random_colour(ui);
						image_manager.random_image(ui);

						image_manager.blur_x(ui);
						screen.clear_blackout();
					}
					'x' => {
						screen.random_colour(ui);
						image_manager.random_image(ui);

						image_manager.blur_y(ui);
						screen.clear_blackout();
					}
					'O' => {
						image_manager.blur_x(ui);
						screen.clear_blackout();
					}
					'X' => {
						image_manager.blur_y(ui);
						screen.clear_blackout();
					}
					':' => {
						screen.random_colour(ui);
						screen.clear_blackout();
					}
					'+' => {
						// blur x?
						image_manager.blur_x(ui);
						screen.blackout();
					}
					'|' => {
						screen.short_blackout();
						screen.random_colour(ui);

						// check this
						image_manager.random_image(ui);
					}
					'*' => {
						image_manager.random_image(ui);

						screen.clear_blackout();
					}
					'=' => {
						image_manager.random_image(ui);

						let length = song.remaining_beat_time(new_index);
						screen.fade_random(duration_to_secs(length), ui);
						screen.clear_blackout();
					}
					'~' => {
						let length = song.remaining_beat_time(new_index);
						screen.fade_random(duration_to_secs(length), ui);
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

				ui.update_time(beat_time as i32);
				ui.update_beat(new_index);
			}
		}
	}

	fn get_song_index<T: AsRef<str>>(&self, title: T) -> Option<usize> {
		self.songs
			.iter()
			.position(|ref song| song.title == title.as_ref())
	}
}

// TODO: guarantee that this will not be out of bounds for the song
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum BeatIndex {
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
					song.buildup_rhythm.push('.');
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
				((duration_to_secs(beat_time) / duration_to_secs(self.loop_beat_length)) as usize) % self.rhythm.len(),
			)
		} else {
			BeatIndex::Buildup(
				(duration_to_secs(beat_time) / duration_to_secs(self.buildup_beat_length)) as usize,
			)
		}
	}

	fn get_beat(&self, beat_index: BeatIndex) -> char {
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

		ui.update_song(self);

		sink
	}

	// Fun fact: multiplication isn't commutative for Duration * u32
	fn remaining_beat_time(&self, beat_index: BeatIndex) -> Duration {
		let buildup_duration = if let BeatIndex::Buildup(idx) = beat_index {
			let remaining = self.buildup_rhythm.split_at(idx).1;
			// Find position of first non '.'
			if let Some(index) = remaining.iter().position(|&beat| beat != '.') {
				return self.buildup_beat_length * index as u32;
			} else {
				self.buildup_beat_length * remaining.len() as u32
			}
		} else {
			Duration::new(0, 0)
		};

		let idx = if let BeatIndex::Loop(idx) = beat_index {
			idx % self.rhythm.len()
		} else {
			0
		};

		let (before, remaining) = self.rhythm.split_at(idx);
		// Find position of first non '.'
		if let Some(index) = remaining.iter().position(|&beat| beat != '.') {
			return self.loop_beat_length * index as u32 + buildup_duration;
		} else {
			// The next one is after the loop, if it exists
			let loop_duration = self.loop_beat_length * remaining.len() as u32 + buildup_duration;
			
			if let Some(index) = before.iter().position(|&beat| beat != '.') {
				return self.loop_beat_length * index as u32 + loop_duration;
			}

			return self.loop_beat_length * before.len() as u32 + loop_duration;

			//unreachable!("There must have been a beat right? Unless this is being called for no reason")
		}
	}
}
