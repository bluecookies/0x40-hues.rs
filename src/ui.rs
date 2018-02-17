use std::fmt;

use sdl2::pixels::Color as Colour;
use sdl2::render::{Canvas, RenderTarget, Texture, TextureCreator, TextureQuery};
use sdl2::rect::Rect;
use sdl2::video::WindowContext;

use sdl2::ttf::Font;

use Result;

struct HexNum(i32);

impl fmt::Display for HexNum {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let val = self.0;
		let width = f.width().unwrap_or(0) + 2;
		if val >= 0 {
			write!(f, "${:#01$x}", val, width)
		} else {
			write!(f, "-{:#01$x}", -val, width)
		}
	}
}

pub struct TextUi {
	texture: Texture,
	rect: Rect,
}

impl TextUi {
	pub fn create<T: AsRef<str>, Target>(
		text: T,
		font: &Font,
		texture_creator: &TextureCreator<Target>,
	) -> Result<Self> {
		let surface = font.render(text.as_ref())
			.blended(Colour::RGBA(0, 0, 0, 255))?;
		let texture = texture_creator.create_texture_from_surface(&surface)?;
		let TextureQuery { width, height, .. } = texture.query();
		let rect = Rect::new(0, 0, width, height);

		Ok(TextUi { texture, rect })
	}

	pub fn draw<T: RenderTarget>(&self, canvas: &mut Canvas<T>) -> Result<()> {
		canvas.copy(&self.texture, None, Some(self.rect))?;
		Ok(())
	}

	pub fn centre(&mut self, x: i32, y: i32, w: u32, h: u32) {
		//center_on(rect.center())
		let x = x + (w - self.rect.width()) as i32 / 2;
		let y = y + (h - self.rect.height()) as i32 / 2;

		self.rect.reposition((x, y));
	}

	pub fn set_pos(&mut self, x: i32, y: i32) {
		self.rect.reposition((x, y));
	}

	pub fn set_text<T: AsRef<str>, Target>(
		&mut self,
		text: T,
		font: &Font,
		texture_creator: &TextureCreator<Target>,
	) -> Result<()> {
		let surface = font.render(text.as_ref())
			.blended(Colour::RGBA(0, 0, 0, 255))?;
		let texture = texture_creator.create_texture_from_surface(&surface)?;
		let TextureQuery { width, height, .. } = texture.query();
		let x = self.rect.left();
		let y = self.rect.top();

		self.texture = texture;
		self.rect = Rect::new(x, y, width, height);

		Ok(())
	}
}

pub trait UiLayout {
	fn update_mode(&mut self, full_auto: bool);
	fn update_time(&mut self, time: i32);
	fn update_beat(&mut self, beat: i32);
	fn update_image(&mut self, image_name: &str);
	fn update_colour(&mut self, index: usize, name: &str);
	fn update_x_blur(&mut self, x: f64);
	fn update_y_blur(&mut self, y: f64);
	fn update_song(&mut self, song_name: &str);

	fn draw<T: RenderTarget>(&self, canvas: &mut Canvas<T>) -> Result<()>;
}

pub struct BasicUi<'a> {
	font: &'a Font<'a, 'static>,
	texture_creator: &'a TextureCreator<WindowContext>,

	mode_text: TextUi,

	image_text: TextUi,
	timer_text: TextUi,
	beat_text: TextUi,

	x_blur_text: TextUi,
	y_blur_text: TextUi,

	colour_index_text: TextUi,
	colour_name_text: TextUi,

	version_text: TextUi,

	song_text: TextUi,
}

impl<'a> BasicUi<'a> {
	pub fn new(
		font: &'a Font<'a, 'static>,
		texture_creator: &'a TextureCreator<WindowContext>,
	) -> Self {
		let mut mode_text = TextUi::create("M=FULL AUTO", &font, &texture_creator).unwrap();
		mode_text.set_pos(0, 576);

		let mut image_text = TextUi::create("I=", &font, &texture_creator).unwrap();
		image_text.set_pos(0, 588);
		let mut timer_text = TextUi::create("T=$0x00000", &font, &texture_creator).unwrap();
		timer_text.set_pos(0, 600);
		let mut beat_text = TextUi::create("B=$0x0000", &font, &texture_creator).unwrap();
		beat_text.set_pos(0, 612);

		let mut x_blur_text = TextUi::create("X=$0x00", &font, &texture_creator).unwrap();
		x_blur_text.set_pos(0, 624);
		let mut y_blur_text = TextUi::create("Y=$0x00", &font, &texture_creator).unwrap();
		y_blur_text.set_pos(0, 636);

		let mut colour_index_text = TextUi::create("C=$0x00", &font, &texture_creator).unwrap();
		colour_index_text.set_pos(0, 648);
		let mut colour_name_text = TextUi::create("BLACK", &font, &texture_creator).unwrap();
		colour_name_text.set_pos(0, 672);

		let mut version_text = TextUi::create("V=$1", &font, &texture_creator).unwrap();
		version_text.set_pos(0, 660);

		// Got to be careful, sdl_ttf doesn't like empty strings
		let mut song_text = TextUi::create(" ", &font, &texture_creator).unwrap();
		song_text.set_pos(0, 684);

		BasicUi {
			font,
			texture_creator,

			mode_text,

			image_text,
			timer_text,
			beat_text,

			x_blur_text,
			y_blur_text,

			colour_index_text,
			colour_name_text,

			version_text,

			song_text,
		}
	}
}

impl<'a> UiLayout for BasicUi<'a> {
	fn update_mode(&mut self, full_auto: bool) {
		let text = if full_auto { "FULL AUTO" } else { "NORMAL" };
		self.mode_text
			.set_text(format!("M={}", text), self.font, self.texture_creator)
			.unwrap();
	}
	fn update_time(&mut self, time: i32) {
		self.timer_text
			.set_text(
				format!("T={:5}", HexNum(time)),
				self.font,
				self.texture_creator,
			)
			.unwrap();
	}

	fn update_beat(&mut self, beat: i32) {
		self.beat_text
			.set_text(
				format!("B={:4}", HexNum(beat)),
				self.font,
				self.texture_creator,
			)
			.unwrap();
	}

	fn update_image(&mut self, image_name: &str) {
		self.image_text
			.set_text(
				format!("I={}", image_name).to_uppercase(),
				self.font,
				self.texture_creator,
			)
			.unwrap();
	}

	fn update_colour(&mut self, index: usize, name: &str) {
		self.colour_index_text
			.set_text(
				format!("C={:2}", HexNum(index as i32)),
				self.font,
				self.texture_creator,
			)
			.unwrap();
		self.colour_name_text
			.set_text(name.to_uppercase(), self.font, self.texture_creator)
			.unwrap();
	}

	fn update_x_blur(&mut self, x: f64) {
		let x = if x >= 1.0 { 255 } else { (x * 256.0) as i32 };
		self.x_blur_text
			.set_text(
				format!("X={:2}", HexNum(x)),
				self.font,
				self.texture_creator,
			)
			.unwrap();
	}

	fn update_y_blur(&mut self, y: f64) {
		let y = if y >= 1.0 { 255 } else { (y * 256.0) as i32 };
		self.y_blur_text
			.set_text(
				format!("Y={:2}", HexNum(y)),
				self.font,
				self.texture_creator,
			)
			.unwrap();
	}

	fn update_song(&mut self, song_name: &str) {
		self.song_text
			.set_text(song_name.to_uppercase(), self.font, self.texture_creator)
			.unwrap();
	}

	fn draw<T: RenderTarget>(&self, canvas: &mut Canvas<T>) -> Result<()> {
		self.mode_text.draw(canvas)?;

		self.image_text.draw(canvas)?;
		self.timer_text.draw(canvas)?;
		self.beat_text.draw(canvas)?;

		self.x_blur_text.draw(canvas)?;
		self.y_blur_text.draw(canvas)?;

		self.colour_index_text.draw(canvas)?;
		self.version_text.draw(canvas)?;
		self.colour_name_text.draw(canvas)?;

		self.song_text.draw(canvas)?;

		Ok(())
	}
}
