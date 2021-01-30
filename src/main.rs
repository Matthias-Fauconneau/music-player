#![feature(default_free_fn, in_band_lifetimes)]
use {anyhow::Error, fehler::throws, std::{default::default, cell::RefCell}};
mod audio;

fn toggle_play_pause(&audio::Output{device, ..}: &audio::Output) -> alsa::Result<()> { device.pause(device.state() == alsa::pcm::State::Running) }

#[throws] async fn play(output: &RefCell<audio::Output>, root: impl AsRef<std::path::Path>) {
	let main_context = glib::MainContext::default();
	main_context.acquire();
	let mpris = mpris_player::MprisPlayer::new("Music".into(), "music-player 0.0.0".into(), "null".into());
	mpris.set_can_pause(true);
	mpris.connect_play_pause(|| toggle_play_pause(&output.borrow()).unwrap() );
	for entry in walkdir::WalkDir::new(root).into_iter().filter(|e| e.as_ref().unwrap().file_type().is_file()) {
		let entry = entry?;
		let path = entry.path();
		let file = symphonia::default::get_probe().format(&symphonia::core::probe::Hint::new().with_extension(path.extension().unwrap().to_str().unwrap()),
																																																		symphonia::core::io::MediaSourceStream::new(Box::new(std::fs::File::open(path)?)), &default(), &default())?;
		let container = file.format;
		let ref codec_parameters = container.default_stream().unwrap().codec_params;
		{let metadata = container.metadata().current().or(file.metadata.current()).unwrap();
			let duration = 0;//container.cues().last().unwrap().start_ts / (codec_parameters.sample_rate.unwrap() as u64);
			let tags = metadata.tags();
			mpris.set_metadata({
				use symphonia::core::meta::StandardTagKey::*;
				mpris_player::Metadata {
					album_artist: Some(tags.iter().filter(|tag| matches!(tag.std_key, Some(AlbumArtist))).map(|tag| tag.value.clone()).collect()),
					album: tags.iter().find(|tag| matches!(tag.std_key, Some(Album))).map(|tag| tag.value.clone()),
					title: tags.iter().find(|tag| matches!(tag.std_key, Some(TrackTitle))).map(|tag| tag.value.clone()),
					length: Some(duration as i64),
					//art_url: Some(metadata.artwork),
					.. mpris_player::Metadata::new()
				}
			})}
		let mut decoder = symphonia::default::get_codecs().make(&codec_parameters, &default())?;
		let mut reader = container;
		while let Ok(packet) = reader.next_packet() {
			main_context.iteration(false); // FIXME: DBus poll
			#[throws] async fn write<S: symphonia::core::sample::Sample>(output: &RefCell<audio::Output>, buffer: &symphonia::core::audio::AudioBuffer<S>) where i16: symphonia::core::conv::FromSample<S> {
				assert_eq!(buffer.spec().rate, output.borrow().device.hw_params_current()?.get_rate()?);
				use symphonia::core::audio::Signal;
				let mut samples = buffer.chan(0).into_iter().map(|&v| symphonia::core::conv::FromSample::from_sample(v)).zip(buffer.chan(1).into_iter().map(|&v| symphonia::core::conv::FromSample::from_sample(v)));
				while samples.len() > 0 {
					output.borrow_mut().write(&mut samples)?;
					async_io::Async::new(alsa::PollDescriptors::get(&output.borrow().device)?[0].fd)?.writable().await?;
				}
			}
			{use symphonia::core::audio::AudioBufferRef::*; match decoder.decode(&packet)? {
				S32(buffer) => write(output, &buffer).await,
				F32(buffer) => write(output, &buffer).await,
			}}?;
		}
	}
}

struct Player<'t>(&'t RefCell<audio::Output>);
impl ui::widget::Widget for Player<'_> {
	#[throws] fn paint(&mut self, _: &mut ui::widget::Target) {}
	#[throws] fn event(&mut self, _: ui::widget::size, _: &ui::widget::EventContext, event: &ui::widget::Event) -> bool {
		match event {
			ui::widget::Event::Key{key:' '} => toggle_play_pause(self.0.borrow()),
			_ => {}
		}
		false
	}
}

#[throws] fn main() {
	let output = RefCell::new(audio::Output::new()?);
	let mut app = ui::app::App::new(Player(&output))?;
	use futures_lite::FutureExt;
	async_io::block_on(async { app.display().await; Ok(()) }.or(play(&output, ".")))?
}
