use std::fmt;
use std::time::{Instant, Duration};

use rand::{Rng, thread_rng as rng};

use sdl2::video::WindowContext;

use rodio;
use rodio::{Endpoint, Sink, Source};

use duration_to_secs;
use AudioData;
use Screen;
use ui::UiLayout;
use images::ImageManager;

use ::Result;

// Make this internal, take a SongLoader from loader
pub struct Song {
	pub name: String,
	pub title: String,
	pub source: Option<String>,
	pub rhythm: Vec<u8>,

	pub buildup: Option<String>,
	pub buildup_rhythm: Vec<u8>,

	// Length of beat
	pub loop_beat_length: Duration,
	pub buildup_beat_length: Duration,

	// Total length of loop/duration
	pub loop_duration: Duration,
	pub buildup_duration: Duration,

	pub loop_audio: AudioData,
	pub buildup_audio: Option<AudioData>,
}

pub struct SongManager {
	songs: Vec<Song>,
	curr_index: Option<usize>,

	music_track: Sink,
	beat_time: Instant,

	beat_index: Option<BeatIndex>,

	endpoint: Endpoint
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
		self.get_song_index(name).map(|index| {
			self.beat_time = Instant::now();
			self.beat_index = None;

			self.music_track = self.songs[index].play(&self.endpoint, ui);

			self.curr_index = Some(index);
		}).ok_or_else(|| "No song.".into())
	}

	pub fn play_random<S: UiLayout>(&mut self, ui: &mut S) {
		let index = rng().gen_range(0, self.songs.len());
		self.beat_time = Instant::now();
		self.beat_index = None;

		self.music_track = self.songs[index].play(&self.endpoint, ui);

		self.curr_index = Some(index);
	}

	pub fn prev_song<T: UiLayout>(&mut self, ui: &mut T) {
		let length = self.songs.len();
		let index = self.curr_index.map_or(0, move |index| (index + length - 1) % length);
		
		self.music_track = self.songs[index].play(&self.endpoint, ui);

		self.beat_time = Instant::now();
		self.beat_index = None;

		self.curr_index = Some(index);
	}

	pub fn next_song<T: UiLayout>(&mut self, ui: &mut T) {
		let length = self.songs.len();
		let index = self.curr_index.map_or(0, move |index| (index + 1) % length);
		
		self.music_track = self.songs[index].play(&self.endpoint, ui);

		self.beat_time = Instant::now();
		self.beat_index = None;

		self.curr_index = Some(index);
	}

	pub fn update_beat<S: UiLayout>(&mut self, screen: &mut Screen, image_manager: &mut ImageManager<WindowContext>, ui: &mut S) {
		if let Some(index) = self.curr_index {
			let song = &self.songs[index];

			let new_index = song.get_beat_index(self.beat_time.elapsed());
			if  self.beat_index != Some(new_index) {
				match song.get_beat(new_index) {
					b'.' => {},
					b'-' => {
						screen.random_colour(ui);
						image_manager.random_image(ui);

						screen.clear_blackout();
					},
					b'o' => {
						screen.random_colour(ui);
						image_manager.random_image(ui);

						image_manager.blur_x(ui);
						screen.clear_blackout();
					},
					b'x' => {
						screen.random_colour(ui);
						image_manager.random_image(ui);

						image_manager.blur_y(ui);
						screen.clear_blackout();
					},
					b'O' => {
						image_manager.blur_x(ui);
						screen.clear_blackout();
					},
					b'X' => {
						image_manager.blur_y(ui);
						screen.clear_blackout();
					},
					b':' => {
						screen.random_colour(ui);
						screen.clear_blackout();
					},
					b'+' => {
						screen.blackout();
					},
					b'=' => {
						image_manager.random_image(ui);
						
						// colourfade
						println!("TODO: colour fade");
						screen.clear_blackout();
					},
					b'~' => {
						// colour fade
						println!("TODO: colour fade");
						screen.clear_blackout();
					},
					ch => println!("TODO: {}", ch)
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
					Some(BeatIndex::Buildup(idx)) =>	idx as i32 - song.buildup_rhythm.len() as i32,
					None => 0
				};

				ui.update_time(beat_time as i32);
				ui.update_beat(beat_index);
			}
		}
	}


	fn get_song_index<T: AsRef<str>>(&self, title: T) -> Option<usize> {
		self.songs.iter().position(|ref song| song.title == title.as_ref())
	}
}


#[derive(Debug, PartialEq, Copy, Clone)]
enum BeatIndex {
	Buildup(usize),
	Loop(usize)
}

impl fmt::Debug for Song {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
