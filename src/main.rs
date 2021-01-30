#![feature(default_free_fn, in_band_lifetimes)]
use {anyhow::Error, fehler::throws, std::cell::RefCell};
mod audio;

#[throws] async fn play(output: &RefCell<audio::Output>, path: &std::path::Path) {
	use std::default::default;
	let mut reader = symphonia::default::get_probe().format(&symphonia::core::probe::Hint::new().with_extension(path.extension().unwrap().to_str().unwrap()),
																																															symphonia::core::io::MediaSourceStream::new(Box::new(std::fs::File::open(path)?)), &default(), &default())?.format;
	let stream = reader.default_stream().unwrap();
	let mut decoder = symphonia::default::get_codecs().make(&stream.codec_params, &default())?;
	while let Ok(packet) = reader.next_packet() {
		#[throws] async fn write<S: symphonia::core::sample::Sample>(output: &RefCell<audio::Output>, buffer: &symphonia::core::audio::AudioBuffer<S>) where i16: symphonia::core::conv::FromSample<S> {
			assert_eq!(buffer.spec().rate, output.borrow().device.hw_params_current()?.get_rate()?);
			use symphonia::core::audio::Signal;
			let mut samples = buffer.chan(0).into_iter().map(|&v| symphonia::core::conv::FromSample::from_sample(v)).zip(buffer.chan(1).into_iter().map(|&v| symphonia::core::conv::FromSample::from_sample(v)));
			while samples.len() > 0 {	 output.borrow_mut().write(&mut samples)?; async_io::Async::new(alsa::PollDescriptors::get(&output.borrow().device)?[0].fd)?.writable().await?; }
		}
		{use symphonia::core::audio::AudioBufferRef::*; match decoder.decode(&packet)? {
			S32(buffer) => write(output, &buffer).await,
			F32(buffer) => write(output, &buffer).await,
		}}?;
	}
}

#[throws] async fn play_all(output: &RefCell<audio::Output>, root: impl AsRef<std::path::Path>) {
	for entry in walkdir::WalkDir::new(root).into_iter().filter(|e| e.as_ref().unwrap().file_type().is_file()) { play(output, entry?.path()).await? }
}

struct Player<'t>(&'t RefCell<audio::Output>);
impl ui::widget::Widget for Player<'_> {
	#[throws] fn paint(&mut self, _: &mut ui::widget::Target) {}
	#[throws] fn event(&mut self, _: ui::widget::size, _: &ui::widget::EventContext, event: &ui::widget::Event) -> bool {
		match event {
			ui::widget::Event::Key{key:' '} => {let ref device = self.0.borrow().device; device.pause(device.state() == alsa::pcm::State::Running)?;}
			_ => {},
		}
		false
	}
}

#[throws] fn main() {
	let output = RefCell::new(audio::Output::new()?);
	let mut app = ui::app::App::new(Player(&output))?;
	use futures_lite::FutureExt;
	async_io::block_on(async { app.display().await; Ok(()) }.or(play_all(&output, ".")))?
}
