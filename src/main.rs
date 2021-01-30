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

#[throws] fn play(output: &mut audio::Output, entry: &std::fs::DirEntry) {
	use std::default::default;
	let mut reader = symphonia::default::get_probe().format(&symphonia::core::probe::Hint::new().with_extension(entry.path().extension().unwrap().to_str().unwrap()),
																																															symphonia::core::io::MediaSourceStream::new(Box::new(std::fs::File::open(entry.path())?)), &default(), &default())?.format;
	let stream = reader.default_stream().unwrap();
	let mut decoder = symphonia::default::get_codecs().make(&stream.codec_params, &default())?;
	while let Ok(packet) = reader.next_packet() {
		#[throws] fn write<S: symphonia::core::sample::Sample>(output: &mut audio::Output, buffer: &symphonia::core::audio::AudioBuffer<S>) where i16: symphonia::core::conv::FromSample<S> {
			assert_eq!(buffer.spec().rate, output.device.hw_params_current()?.get_rate()?);
			use symphonia::core::audio::Signal;
			let mut samples = buffer.chan(0).into_iter().map(|&v| symphonia::core::conv::FromSample::from_sample(v)).zip(buffer.chan(1).into_iter().map(|&v| symphonia::core::conv::FromSample::from_sample(v)));
			while samples.len() > 0 {	 output.write(&mut samples)?; alsa::poll::poll(&mut alsa::PollDescriptors::get(&output.device)?, -1)?; } //alsa::poll::poll(&mut output.poll, -1)?; }
		}
		{use symphonia::core::audio::AudioBufferRef::*; match decoder.decode(&packet)? {
			S32(buffer) => write(output, &buffer),
			F32(buffer) => write(output, &buffer),
		}}?;
	}
}

#[throws] fn main() {
	let mut output = audio::Output::new()?;
	visit(&std::env::current_dir()?, &mut move |entry| play(&mut output, entry))?
}
