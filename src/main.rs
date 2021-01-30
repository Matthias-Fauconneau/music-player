#![feature(default_free_fn)]
use {anyhow::{Error, Result}, fehler::throws};

#[throws] fn visit(path: &std::path::Path, f: &mut impl FnMut(&std::fs::DirEntry) -> Result<()>) {
	if path.is_dir() {
		for entry in std::fs::read_dir(path)? {
			let entry = entry?;
			let path = entry.path();
			if path.is_dir() { visit(&path, f)?; } else { f(&entry)?; }
		}
	}
}

mod audio;

#[throws] fn play(mut output: &mut audio::Output, entry: &std::fs::DirEntry) {
	use std::default::default;
	let mut reader = symphonia::default::get_probe().format(&symphonia::core::probe::Hint::new().with_extension(entry.path().extension().unwrap().to_str().unwrap()),
																																															symphonia::core::io::MediaSourceStream::new(Box::new(std::fs::File::open(entry.path())?)), &default(), &default())?.format;
	let stream = reader.default_stream().unwrap();
	let mut decoder = symphonia::default::get_codecs().make(&stream.codec_params, &default())?;
	//assert_eq!(reader.streaminfo().sample_rate, output.rate);
	while let Ok(packet) = reader.next_packet() {
		match decoder.decode(&packet)? {
			symphonia::core::audio::AudioBufferRef::S32(buffer) => {
				use symphonia::core::audio::Signal;
				//use itertools::Itertools; let mut samples = buffer.chan(0).into_iter().interleave_shortest(buffer.chan(1)).map(|v| v as i16); // impl ExactSizeIterator
				//let mut samples = (0..buffer.frames()).flat_map(|i| [buffer.chan(0)[i], buffer.chan(1)[i]].into_iter()).map(|v| v as i16); // impl ExactSizeIterator
				//let mut samples = (0..buffer.frames()).map(|i| [buffer.chan(0)[i], buffer.chan(1)[i]]
				let mut samples = buffer.chan(0).into_iter().map(|&v| v as i16).zip(buffer.chan(1).into_iter().map(|&v| v as i16));
				while samples.len() > 0 { 	audio::write(&mut output, &mut samples); }
			},
			_ => unimplemented!(),
		}
	}
}

#[throws] fn main() {
	let mut output = audio::output()?;
	visit(&std::env::current_dir()?, &mut move |entry| play(&mut output, entry))?
}
