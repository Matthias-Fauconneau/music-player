#![feature(default_free_fn)]
use {anyhow::Error, fehler::throws};
mod audio;

#[throws] async fn play(output: &mut audio::Output, path: &std::path::Path) {
	use std::default::default;
	let mut reader = symphonia::default::get_probe().format(&symphonia::core::probe::Hint::new().with_extension(path.extension().unwrap().to_str().unwrap()),
																																															symphonia::core::io::MediaSourceStream::new(Box::new(std::fs::File::open(path)?)), &default(), &default())?.format;
	let stream = reader.default_stream().unwrap();
	let mut decoder = symphonia::default::get_codecs().make(&stream.codec_params, &default())?;
	while let Ok(packet) = reader.next_packet() {
		#[throws] async fn write<S: symphonia::core::sample::Sample>(output: &mut audio::Output, buffer: &symphonia::core::audio::AudioBuffer<S>) where i16: symphonia::core::conv::FromSample<S> {
			assert_eq!(buffer.spec().rate, output.device.hw_params_current()?.get_rate()?);
			use symphonia::core::audio::Signal;
			let mut samples = buffer.chan(0).into_iter().map(|&v| symphonia::core::conv::FromSample::from_sample(v)).zip(buffer.chan(1).into_iter().map(|&v| symphonia::core::conv::FromSample::from_sample(v)));
			while samples.len() > 0 {	 output.write(&mut samples)?; async_io::Async::new(alsa::PollDescriptors::get(&output.device)?[0].fd)?.writable().await?; }
		}
		{use symphonia::core::audio::AudioBufferRef::*; match decoder.decode(&packet)? {
			S32(buffer) => write(output, &buffer).await,
			F32(buffer) => write(output, &buffer).await,
		}}?;
	}
}

#[throws] async fn play_all(output: &mut audio::Output, root: impl AsRef<std::path::Path>) { for entry in walkdir::WalkDir::new(root).into_iter().filter(|e| e.as_ref().unwrap().file_type().is_file()) { play(output, entry?.path()).await? } }

struct Empty;
impl ui::widget::Widget for Empty {
	#[throws] fn paint(&mut self, _: &mut ui::widget::Target) {}
}

#[throws] fn main() {
	let mut output = audio::Output::new()?;
	let mut app = ui::app::App::new(Empty)?;
	use futures_lite::FutureExt;
	async_io::block_on(async { app.display().await; Ok(()) }.or(play_all(&mut output, ".")))?
}
