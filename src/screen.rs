use std::time::Instant;

use sdl2::pixels::{Color as Colour, PixelFormatEnum};
use sdl2::surface::Surface;
use sdl2::render::{Texture, TextureCreator, WindowCanvas as Canvas};

use rand::{thread_rng as rng, Rng};

use ui::UiLayout;
use duration_to_secs;

pub struct Screen {
	colour: Colour,

	full_black: bool,
	blackout_init: Option<Instant>,
	blackout_texture: Texture,

	fade_init: Option<Instant>,
	fade_colour: Colour,
	fade_texture: Texture,
	fade_end: f64, // seconds
}

impl Screen {
	pub fn new<T>(texture_creator: &TextureCreator<T>) -> Self {
		Screen {
			colour: Colour::RGBA(0x00, 0x00, 0x00, 0xFF),

			// Don't start off with an actual blackout
			full_black: false,
			blackout_init: None,
			blackout_texture: {
				// unsure whether large or small texture is good
				let mut surface = Surface::new(1280, 720, PixelFormatEnum::RGBA8888).unwrap();
				surface
					.fill_rect(None, Colour::RGBA(0x00, 0x00, 0x00, 0xFF))
					.unwrap();
				texture_creator
					.create_texture_from_surface(surface)
					.unwrap()
			},

			fade_texture: {
				let mut surface = Surface::new(1280, 720, PixelFormatEnum::RGBA8888).unwrap();
				surface
					.fill_rect(None, Colour::RGBA(0xFF, 0xFF, 0xFF, 0xFF))
					.unwrap();
				texture_creator
					.create_texture_from_surface(surface)
					.unwrap()
			},
			fade_init: None,
			fade_end: 1.0,
			fade_colour: Colour::RGBA(0xFF, 0xFF, 0xFF, 0xFF),
		}
	}

	pub fn clear(&mut self, canvas: &mut Canvas) {
		canvas.set_draw_color(self.colour);
		canvas.clear();

		if let Some(start) = self.fade_init {
			let fade = duration_to_secs(start.elapsed()) / self.fade_end;

			if fade >= 1.0 {
				self.colour = self.fade_colour;
				self.fade_init = None;
			} else {
				// Canvas Blendmode should be Blend
				// dstRGB = (srcRGB * srcA) + (dstRGB * (1 - srcA))
				// dstA = srcA + (dstA * (1 - srcA)) = 1
				let alpha = (fade * 256.0) as u8;
				self.fade_texture.set_alpha_mod(alpha);
				canvas.copy(&self.fade_texture, None, None).unwrap();
			}
		}
	}

	// I think I might need to change these to trait objects later if I'm serious
	pub fn random_colour<T: UiLayout>(&mut self, ui: &mut T) {
		let idx = rng().gen_range(0x00, HUES.len());
		let (hue, name) = HUES[idx];

		//ui.update_colour_index(idx);
		//ui.update_colour_name(name);
		ui.update_colour(idx, name);
		self.colour = hue;
	}

	// Length is in seconds
	pub fn fade_random<T: UiLayout>(&mut self, length: f64, ui: &mut T) {
		let idx = rng().gen_range(0x00, HUES.len());
		let (Colour {r, g, b, a}, name) = HUES[idx];

		ui.update_colour(idx, name);
		self.fade_texture.set_color_mod(r, g, b);
		self.fade_colour = Colour { r, g, b, a };
		self.fade_init = Some(Instant::now());
		self.fade_end = length;
	}

	pub fn clear_blackout(&mut self) {
		self.blackout_init = None;
		self.full_black = false;
	}

	pub fn blackout(&mut self) {
		self.blackout_init = Some(Instant::now());
	}

	pub fn short_blackout(&mut self) {
		self.full_black = true;
	}

	pub fn draw(&mut self, canvas: &mut Canvas) {
		if self.full_black {
			canvas.set_draw_color(Colour::RGB(0x00, 0x00, 0x00));
			canvas.fill_rect(None).unwrap();
			return;
		}

		if let Some(start) = self.blackout_init {
			let fade = duration_to_secs(start.elapsed()) * 10.0;
			// Maybe set a flag to check before drawing image
			// TODO: ^ do that
			if fade >= 1.0 {
				canvas.set_draw_color(Colour::RGB(0x00, 0x00, 0x00));
				canvas.fill_rect(None).unwrap();
			} else {
				let alpha = (fade * 256.0) as u8;
				self.blackout_texture.set_alpha_mod(alpha);
				canvas.copy(&self.blackout_texture, None, None).unwrap();
			}
		}
	}
}

// 0x40 hues
// damnit, rustfmt messed this up
static HUES: [(Colour, &str); 0x40] = [
	(
		Colour {
			r: 0x00,
			g: 0x00,
			b: 0x00,
			a: 0xFF,
		},
		"black",
	),
	(
		Colour {
			r: 0x55,
			g: 0x00,
			b: 0x00,
			a: 0xFF,
		},
		"brick",
	),
	(
		Colour {
			r: 0xAA,
			g: 0x00,
			b: 0x00,
			a: 0xFF,
		},
		"crimson",
	),
	(
		Colour {
			r: 0xFF,
			g: 0x00,
			b: 0x00,
			a: 0xFF,
		},
		"red",
	),
	(
		Colour {
			r: 0x00,
			g: 0x55,
			b: 0x00,
			a: 0xFF,
		},
		"turtle",
	),
	(
		Colour {
			r: 0x55,
			g: 0x55,
			b: 0x00,
			a: 0xFF,
		},
		"sludge",
	),
	(
		Colour {
			r: 0xAA,
			g: 0x55,
			b: 0x00,
			a: 0xFF,
		},
		"brown",
	),
	(
		Colour {
			r: 0xFF,
			g: 0x55,
			b: 0x00,
			a: 0xFF,
		},
		"orange",
	),
	(
		Colour {
			r: 0x00,
			g: 0xAA,
			b: 0x00,
			a: 0xFF,
		},
		"green",
	),
	(
		Colour {
			r: 0x55,
			g: 0xAA,
			b: 0x00,
			a: 0xFF,
		},
		"grass",
	),
	(
		Colour {
			r: 0xAA,
			g: 0xAA,
			b: 0x00,
			a: 0xFF,
		},
		"maize",
	),
	(
		Colour {
			r: 0xFF,
			g: 0xAA,
			b: 0x00,
			a: 0xFF,
		},
		"citrus",
	),
	(
		Colour {
			r: 0x00,
			g: 0xFF,
			b: 0x00,
			a: 0xFF,
		},
		"lime",
	),
	(
		Colour {
			r: 0x55,
			g: 0xFF,
			b: 0x00,
			a: 0xFF,
		},
		"leaf",
	),
	(
		Colour {
			r: 0xAA,
			g: 0xFF,
			b: 0x00,
			a: 0xFF,
		},
		"chartreuse",
	),
	(
		Colour {
			r: 0xFF,
			g: 0xFF,
			b: 0x00,
			a: 0xFF,
		},
		"yellow",
	),
	(
		Colour {
			r: 0x00,
			g: 0x00,
			b: 0x55,
			a: 0xFF,
		},
		"midnight",
	),
	(
		Colour {
			r: 0x55,
			g: 0x00,
			b: 0x55,
			a: 0xFF,
		},
		"plum",
	),
	(
		Colour {
			r: 0xAA,
			g: 0x00,
			b: 0x55,
			a: 0xFF,
		},
		"pomegranate",
	),
	(
		Colour {
			r: 0xFF,
			g: 0x00,
			b: 0x55,
			a: 0xFF,
		},
		"rose",
	),
	(
		Colour {
			r: 0x00,
			g: 0x55,
			b: 0x55,
			a: 0xFF,
		},
		"swamp",
	),
	(
		Colour {
			r: 0x55,
			g: 0x55,
			b: 0x55,
			a: 0xFF,
		},
		"dust",
	),
	(
		Colour {
			r: 0xAA,
			g: 0x55,
			b: 0x55,
			a: 0xFF,
		},
		"dirt",
	),
	(
		Colour {
			r: 0xFF,
			g: 0x55,
			b: 0x55,
			a: 0xFF,
		},
		"blossom",
	),
	(
		Colour {
			r: 0x00,
			g: 0xAA,
			b: 0x55,
			a: 0xFF,
		},
		"sea",
	),
	(
		Colour {
			r: 0x55,
			g: 0xAA,
			b: 0x55,
			a: 0xFF,
		},
		"ill",
	),
	(
		Colour {
			r: 0xAA,
			g: 0xAA,
			b: 0x55,
			a: 0xFF,
		},
		"haze",
	),
	(
		Colour {
			r: 0xFF,
			g: 0xAA,
			b: 0x55,
			a: 0xFF,
		},
		"peach",
	),
	(
		Colour {
			r: 0x00,
			g: 0xFF,
			b: 0x55,
			a: 0xFF,
		},
		"spring",
	),
	(
		Colour {
			r: 0x55,
			g: 0xFF,
			b: 0x55,
			a: 0xFF,
		},
		"mantis",
	),
	(
		Colour {
			r: 0xAA,
			g: 0xFF,
			b: 0x55,
			a: 0xFF,
		},
		"brilliant",
	),
	(
		Colour {
			r: 0xFF,
			g: 0xFF,
			b: 0x55,
			a: 0xFF,
		},
		"canary",
	),
	(
		Colour {
			r: 0x00,
			g: 0x00,
			b: 0xAA,
			a: 0xFF,
		},
		"navy",
	),
	(
		Colour {
			r: 0x55,
			g: 0x00,
			b: 0xAA,
			a: 0xFF,
		},
		"grape",
	),
	(
		Colour {
			r: 0xAA,
			g: 0x00,
			b: 0xAA,
			a: 0xFF,
		},
		"mauve",
	),
	(
		Colour {
			r: 0xFF,
			g: 0x00,
			b: 0xAA,
			a: 0xFF,
		},
		"purple",
	),
	(
		Colour {
			r: 0x00,
			g: 0x55,
			b: 0xAA,
			a: 0xFF,
		},
		"cornflower",
	),
	(
		Colour {
			r: 0x55,
			g: 0x55,
			b: 0xAA,
			a: 0xFF,
		},
		"deep",
	),
	(
		Colour {
			r: 0xAA,
			g: 0x55,
			b: 0xAA,
			a: 0xFF,
		},
		"lilac",
	),
	(
		Colour {
			r: 0xFF,
			g: 0x55,
			b: 0xAA,
			a: 0xFF,
		},
		"lavender",
	),
	(
		Colour {
			r: 0x00,
			g: 0xAA,
			b: 0xAA,
			a: 0xFF,
		},
		"aqua",
	),
	(
		Colour {
			r: 0x55,
			g: 0xAA,
			b: 0xAA,
			a: 0xFF,
		},
		"steel",
	),
	(
		Colour {
			r: 0xAA,
			g: 0xAA,
			b: 0xAA,
			a: 0xFF,
		},
		"grey",
	),
	(
		Colour {
			r: 0xFF,
			g: 0xAA,
			b: 0xAA,
			a: 0xFF,
		},
		"pink",
	),
	(
		Colour {
			r: 0x00,
			g: 0xFF,
			b: 0xAA,
			a: 0xFF,
		},
		"bay",
	),
	(
		Colour {
			r: 0x55,
			g: 0xFF,
			b: 0xAA,
			a: 0xFF,
		},
		"marina",
	),
	(
		Colour {
			r: 0xAA,
			g: 0xFF,
			b: 0xAA,
			a: 0xFF,
		},
		"tornado",
	),
	(
		Colour {
			r: 0xFF,
			g: 0xFF,
			b: 0xAA,
			a: 0xFF,
		},
		"saltine",
	),
	(
		Colour {
			r: 0x00,
			g: 0x00,
			b: 0xFF,
			a: 0xFF,
		},
		"blue",
	),
	(
		Colour {
			r: 0x55,
			g: 0x00,
			b: 0xFF,
			a: 0xFF,
		},
		"twilight",
	),
	(
		Colour {
			r: 0xAA,
			g: 0x00,
			b: 0xFF,
			a: 0xFF,
		},
		"orchid",
	),
	(
		Colour {
			r: 0xFF,
			g: 0x00,
			b: 0xFF,
			a: 0xFF,
		},
		"magenta",
	),
	(
		Colour {
			r: 0x00,
			g: 0x55,
			b: 0xFF,
			a: 0xFF,
		},
		"azure",
	),
	(
		Colour {
			r: 0x55,
			g: 0x55,
			b: 0xFF,
			a: 0xFF,
		},
		"liberty",
	),
	(
		Colour {
			r: 0xAA,
			g: 0x55,
			b: 0xFF,
			a: 0xFF,
		},
		"royalty",
	),
	(
		Colour {
			r: 0xFF,
			g: 0x55,
			b: 0xFF,
			a: 0xFF,
		},
		"thistle",
	),
	(
		Colour {
			r: 0x00,
			g: 0xAA,
			b: 0xFF,
			a: 0xFF,
		},
		"ocean",
	),
	(
		Colour {
			r: 0x55,
			g: 0xAA,
			b: 0xFF,
			a: 0xFF,
		},
		"sky",
	),
	(
		Colour {
			r: 0xAA,
			g: 0xAA,
			b: 0xFF,
			a: 0xFF,
		},
		"periwinkle",
	),
	(
		Colour {
			r: 0xFF,
			g: 0xAA,
			b: 0xFF,
			a: 0xFF,
		},
		"carnation",
	),
	(
		Colour {
			r: 0x00,
			g: 0xFF,
			b: 0xFF,
			a: 0xFF,
		},
		"cyan",
	),
	(
		Colour {
			r: 0x55,
			g: 0xFF,
			b: 0xFF,
			a: 0xFF,
		},
		"turquoise",
	),
	(
		Colour {
			r: 0xAA,
			g: 0xFF,
			b: 0xFF,
			a: 0xFF,
		},
		"powder",
	),
	(
		Colour {
			r: 0xFF,
			g: 0xFF,
			b: 0xFF,
			a: 0xFF,
		},
		"white",
	),
];
