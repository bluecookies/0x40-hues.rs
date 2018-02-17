use std::time::Instant;

use rand::{thread_rng as rng, Rng};

use sdl2::rect::Rect;
use sdl2::render::{Texture, TextureCreator, WindowCanvas as Canvas};

use loader::ImageLoader;
use ui::UiLayout;
use Result;

use duration_to_secs;

pub struct ImageManager<'a, Target: 'a> {
	images: Vec<Image>,
	curr_index: Option<usize>,

	full_auto: bool,

	blur: Blur,

	texture_creator: &'a TextureCreator<Target>,
}

impl<'a, Target> ImageManager<'a, Target> {
	pub fn new(texture_creator: &'a TextureCreator<Target>) -> Self {
		ImageManager {
			images: Vec::new(),
			curr_index: None,

			full_auto: true,

			blur: Blur::new(7),

			texture_creator,
		}
	}

	pub fn extend(&mut self, pack: Vec<ImageLoader>) {
		let texture_creator = self.texture_creator;
		self.images.extend(pack.into_iter().map(|image_loader| {
			let texture = texture_creator
				.create_texture_from_surface(&image_loader.data)
				.unwrap();

			Image::from_loader(image_loader, texture)
		}));
	}

	// Ok this isn't actually "random image" but it's not being used anywhere else so it stays like this for now
	pub fn random_image<S: UiLayout>(&mut self, ui: &mut S) {
		if self.full_auto {
			let idx = rng().gen_range(0, self.images.len());
			ui.update_image(&self.images[idx].name);

			self.curr_index = Some(idx);
		}
	}

	pub fn prev_image<S: UiLayout>(&mut self, ui: &mut S) {
		let length = self.images.len();
		let idx = self.curr_index
			.map_or(0, move |index| (index + length - 1) % length);
		ui.update_image(&self.images[idx].name);

		self.curr_index = Some(idx);
		self.full_auto = false;
	}

	pub fn next_image<S: UiLayout>(&mut self, ui: &mut S) {
		let length = self.images.len();
		let idx = self.curr_index.map_or(0, move |index| (index + 1) % length);
		ui.update_image(&self.images[idx].name);

		self.curr_index = Some(idx);
		self.full_auto = false;
	}

	pub fn toggle_full_auto<S: UiLayout>(&mut self, ui: &mut S) {
		self.full_auto = !self.full_auto;

		ui.update_mode(self.full_auto);
	}

	pub fn draw_image<S: UiLayout>(&mut self, canvas: &mut Canvas, ui: &mut S) {
		if let Some(index) = self.curr_index {
			self.images[index].draw(&mut self.blur, canvas, ui).unwrap();
		}
	}

	pub fn blur_x<T: UiLayout>(&mut self, ui: &mut T) {
		self.blur.blur_x(ui);
	}

	pub fn blur_y<T: UiLayout>(&mut self, ui: &mut T) {
		self.blur.blur_y(ui);
	}
}

// Image
struct Image {
	name: String,
	image: Texture,
	fullname: Option<String>,
	source: Option<String>,
	source_other: Option<String>,
}

impl Image {
	fn from_loader(loader: ImageLoader, texture: Texture) -> Self {
		Image {
			name: loader.name,
			image: texture,
			fullname: loader.fullname,
			source: loader.source,
			source_other: loader.source_other,
		}
	}

	// TODO: align
	fn draw<S: UiLayout>(
		&mut self,
		blur: &mut Blur,
		canvas: &mut Canvas,
		ui: &mut S,
	) -> Result<()> {
		match blur.blur_type {
			BlurType::Horizontal => {
				self.image.set_alpha_mod(0xFF / blur.num);

				let factor = blur.factor();
				let dist = blur.dist * factor;

				for x in (0..blur.num).map(|i| 2.0 * i as f64 / (blur.num as f64 - 1.0) - 1.0) {
					let rect = Rect::new((x * dist) as i32, 0, 1280, 720);
					canvas.copy(&self.image, None, Some(rect))?;
				}

				if dist < 1.0 {
					blur.blur_type = BlurType::None;
					ui.update_x_blur(0.0);
				} else {
					ui.update_x_blur(factor);
				}
			}
			BlurType::Vertical => {
				self.image.set_alpha_mod(0xFF / blur.num);

				let factor = blur.factor();
				let dist = blur.dist * factor;

				for y in (0..blur.num).map(|i| 2.0 * i as f64 / (blur.num as f64 - 1.0) - 1.0) {
					let rect = Rect::new(0, (y * dist) as i32, 1280, 720);
					canvas.copy(&self.image, None, Some(rect))?;
				}

				if dist < 1.0 {
					blur.blur_type = BlurType::None;
					ui.update_y_blur(0.0);
				} else {
					ui.update_y_blur(factor);
				}
			}
			BlurType::None => {
				self.image.set_alpha_mod(0xD0);
				canvas.copy(&self.image, None, None)?;
			}
		}
		Ok(())
	}
}

// Blur
struct Blur {
	blur_type: BlurType,
	num: u8,
	dist: f64,
	init: Instant,
}

enum BlurType {
	Horizontal,
	Vertical,
	None,
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
