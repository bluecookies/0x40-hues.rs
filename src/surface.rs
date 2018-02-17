// Done because loading image data into surface in the main thread was too slow
// But there were two issues
// 1. Surface has Rc<SurfaceContext> which isn't thread safe
//   so can't do the usual load surface in thread -> pass it to main thread
// 2. Even if that was possible, IMG_LoadPNG from RWops returns a surface that doesn't
//   own the data (I think?) so in rust, it would have a lifetime bound to the rwops
//  But the RWops itself is representing a stream of some sort, so it has a lifetime bound
//  to the buffer its made from
//  and that's just a bit too annoying to be carrying around all this luggage
// So, this just takes a surface and copies it to something owning

// Also SDL_Surface has some weird refcount thing that's decremented with SDL_FreeSurface
// So.. maybe Surface can be cloned? with a refcount? but no need for the time being

use std::rc::Rc;
use std::ptr::copy_nonoverlapping as memcpy;

use std::os::raw::c_int;

use sdl2::surface::{Surface as STSurface, SurfaceRef};
use sdl2::sys;
use sdl2::get_error as sdl_get_error;

use Result;

// static
// wait why don't I just own this then
pub struct Surface {
	raw: *mut sys::SDL_Surface,
}

impl Drop for Surface {
	#[inline]
	fn drop(&mut self) {
		unsafe {
			sys::SDL_FreeSurface(self.raw);
		}
	}
}

// If needed, something like
//	impl Clone for Surface {
//		fn clone(&self) -> Self {
// 			let raw = &*(self.raw);
// 			raw.refcount += 1;
//			Surface {
//				self.raw.copy()
//			}
// 		}
//	}
// if that compiles

impl AsRef<SurfaceRef> for Surface {
	#[inline]
	fn as_ref(&self) -> &SurfaceRef {
		unsafe { &*(self.raw as *const SurfaceRef) }
	}
}

impl AsMut<SurfaceRef> for Surface {
	#[inline]
	fn as_mut(&mut self) -> &mut SurfaceRef {
		unsafe { &mut *(self.raw as *mut SurfaceRef) }
	}
}

unsafe impl Send for Surface {}

impl Surface {
	pub fn from_surface(surf: STSurface) -> Result<Surface> {
		if Rc::strong_count(&surf.context()) != 2 {
			return Err("Surface is not unique".into());
		}
		let raw = surf.raw();
		let (n, surface) = unsafe {
			let surf = &*raw;
			let pixel_format = &*surf.format;

			let depth = pixel_format.BitsPerPixel as c_int;
			let format = pixel_format.format;

			let n = surf.pitch * surf.h;
			let surface = sys::SDL_CreateRGBSurfaceWithFormat(0, surf.w, surf.h, depth, format);
			if (*surface).pitch != surf.pitch {
				sys::SDL_FreeSurface(surface);
				return Err("Pixel data error".into());
			}
			(n, surface)
		};

		if surface.is_null() {
			return Err(sdl_get_error().into());
		}

		// Maybe a blit is fine
		unsafe {
			memcpy(
				(*raw).pixels as *const u8,
				(*surface).pixels as *mut u8,
				n as usize,
			);
		}

		// drop surf -> drop context -> drop it all

		Ok(Surface { raw: surface })
		/*
			//let context = surf.context();
			//drop(surf);
			let raw = surf.raw();
			// Don't let the rc decrement
			// Wait this leaks
			// mem::forget(surf);
			// Ok make a clone of the rc context, clean up the surface
			// and then, yeah so still needs to be the only copy of surface
			let context = surf.context();
			mem::drop(surf);
			let context = Rc::try_unwrap(context).map_err(|_rc| "Could not get surface context.")?;
			mem::forget(context);

			Ok(Surface { 
				context: SurfaceContext {
					raw,
					buffer: rwops
				}
			})
		*/
	}
}
