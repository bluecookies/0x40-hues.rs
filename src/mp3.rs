extern crate simplemad;

use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use rodio::Source;

unsafe impl<R: Read + Send> Send for Mp3Decoder<R> {}

pub struct Mp3Decoder<R> where R: Read {
    reader: simplemad::Decoder<R>,
    current_frame: simplemad::Frame,
    current_frame_channel: usize,
    current_frame_sample_pos: usize,

    duration: Duration
}

impl<R> Mp3Decoder<R> where R: Read + Seek + Clone {
    pub fn new(mut data: R) -> Mp3Decoder<R> {
        if !is_mp3(data.by_ref()) {
            panic!("Not valid mp3.");
        }

        let duration = {
            let headers = simplemad::Decoder::decode_headers(data.clone()).unwrap();
            headers.filter_map(|r| {
              match r {
                  Ok(f) => Some(f.duration),
                  Err(_) => None,
              }
          }).fold(Duration::new(0, 0), |acc, dtn| acc + dtn)
        };

        let mut reader = simplemad::Decoder::decode(data).unwrap();

        let current_frame = next_frame(&mut reader);

        Mp3Decoder {
            reader: reader,
            current_frame: current_frame,
            current_frame_channel: 0,
            current_frame_sample_pos: 0,

            duration,
        }
    }
}

impl<R> Source for Mp3Decoder<R> where R: Read {
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.current_frame.samples[0].len())
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.current_frame.samples.len() as u16
    }

    #[inline]
    fn samples_rate(&self) -> u32 {
        self.current_frame.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(self.duration)
    }
}

impl<R> Iterator for Mp3Decoder<R> where R: Read {
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        if self.current_frame.samples[0].len() == 0 {
            return None;
        }

        // getting the sample and converting it from fixed step to i16
        let sample = self.current_frame.samples[self.current_frame_channel][self.current_frame_sample_pos].to_raw();
        let sample = sample + (1 << (28 - 16));
        let sample = if sample >= 0x10000000 { 0x10000000 - 1 } else if sample <= -0x10000000 { -0x10000000 } else { sample };
        let sample = sample >> (28 + 1 - 16);
        let sample = sample as i16;

        self.current_frame_channel += 1;

        if self.current_frame_channel < self.current_frame.samples.len() {
            return Some(sample);
        }

        self.current_frame_channel = 0;
        self.current_frame_sample_pos += 1;

        if self.current_frame_sample_pos < self.current_frame.samples[0].len() {
            return Some(sample);
        }
        
        self.current_frame = next_frame(&mut self.reader);
        self.current_frame_channel = 0;
        self.current_frame_sample_pos = 0;

        return Some(sample);
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.current_frame.samples[0].len(), None)
    }
}

/// Returns the next frame of a decoder, ignores errors.
fn next_frame<R>(decoder: &mut simplemad::Decoder<R>) -> simplemad::Frame
                 where R: Read
{
    let frame = decoder.filter_map(|f| f.ok()).next();
    let frame = frame.unwrap_or_else(|| {
        simplemad::Frame {
            sample_rate: 44100,
            bit_rate: 0,
            samples: vec![Vec::new()],
            position: Duration::new(0, 0),
            duration: Duration::new(0, 0),
            layer: Default::default(),
            mode: Default::default(),
        }
    });

    frame
}

/// Returns true if the stream contains MP3 data, then resets it to where it was.
fn is_mp3<R>(mut data: R) -> bool where R: Read + Seek {
    let stream_pos = data.seek(SeekFrom::Current(0)).unwrap();

    if simplemad::Decoder::decode(data.by_ref()).is_err() {
        data.seek(SeekFrom::Start(stream_pos)).unwrap();
        return false;
    }

    data.seek(SeekFrom::Start(stream_pos)).unwrap();
    true
}