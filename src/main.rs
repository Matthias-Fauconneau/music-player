#![feature(default_free_fn, in_band_lifetimes)]
use {fehler::throws, anyhow::Error, std::{default::default, sync::{Arc, Mutex}, ops::{Deref, DerefMut}}};
mod audio;
mod mpris;

fn playing(device: &alsa::PCM) -> bool { device.state() == alsa::pcm::State::Running }
fn toggle_play_pause(device: &alsa::PCM) -> alsa::Result<()> { device.pause(playing(&device)) }

#[throws] async fn write<S: symphonia::core::sample::Sample>(device: &'t Mutex<alsa::PCM>, mut output: &mut alsa::direct::pcm::MmapPlayback<i16>, buffer: &symphonia::core::audio::AudioBuffer<S>) where i16: symphonia::core::conv::FromSample<S> {
	assert_eq!(buffer.spec().rate, device.lock().unwrap().hw_params_current()?.get_rate()?);
	use symphonia::core::audio::Signal;
	let mut samples = buffer.chan(0).into_iter().map(|&v| symphonia::core::conv::FromSample::from_sample(v)).zip(buffer.chan(1).into_iter().map(|&v| symphonia::core::conv::FromSample::from_sample(v)));
	while samples.len() > 0 {
		{let mut device = device.lock().unwrap();
			audio::write(device.deref_mut(), &mut output, &mut samples)?;
			async_io::Async::new(alsa::PollDescriptors::get(device.deref())?[0].fd)?.writable()
		}.await?;
	}
}

struct Player {
	device: Mutex<alsa::PCM>,
	metadata: Mutex<std::collections::HashMap<String, String>>,
}

impl Player {
#[throws] async fn play(&self, mut output: alsa::direct::pcm::MmapPlayback<i16>, root: impl AsRef<std::path::Path>) {
	for entry in walkdir::WalkDir::new(root).into_iter().filter(|e| e.as_ref().unwrap().file_type().is_file()) {
		let entry = entry?;
		let path = entry.path();
		let file = symphonia::default::get_probe().format(&symphonia::core::probe::Hint::new().with_extension(path.extension().unwrap().to_str().unwrap()),
																																																		symphonia::core::io::MediaSourceStream::new(Box::new(std::fs::File::open(path)?)), &default(), &default())?;
		let container = file.format;
		let ref codec_parameters = container.default_stream().unwrap().codec_params;
		{let mut metadata = std::collections::HashMap::new();
			for tag in container.metadata().current().or(file.metadata.current()).unwrap().tags() { if let Some(key) = tag.std_key {
				let key = {use symphonia::core::meta::StandardTagKey::*; match key {
					Artist|SortArtist => "xesam:artist",
					TrackNumber => "xesam:TrackNumber",
					OriginalDate|Date => "xesam:contentCreated",
					Genre => "xesam:genre",
					Bpm => "xesam:audioBPM",
					Album => "xesam:album",
					AlbumArtist => "xesam:albumArtist",
					TrackTitle => "xesam:title",
					Composer|SortComposer => "xesam:composer",
					MediaFormat|Language|Lyrics|Label => "",
					key => panic!("{:?}", key),
				}};
				if !key.is_empty() { metadata.insert(key.to_string(), tag.value.clone()); }
			}}
			*self.metadata.lock().unwrap() = metadata;
		}
		let mut decoder = symphonia::default::get_codecs().make(&codec_parameters, &default())?;
		let mut reader = container;
		while let Ok(packet) = reader.next_packet() {
			{use symphonia::core::audio::AudioBufferRef::*; match decoder.decode(&packet)? {
				S32(buffer) => write(&self.device, &mut output, &buffer).await,
				F32(buffer) => write(&self.device, &mut output, &buffer).await,
			}}?;
		}
	}
}
}

impl ui::widget::Widget for &Player {
	#[throws] fn paint(&mut self, _: &mut ui::widget::Target) {}
	#[throws] fn event(&mut self, _: ui::widget::size, _: &ui::widget::EventContext, event: &ui::widget::Event) -> bool {
		match event {
			ui::widget::Event::Key{key:' '} => toggle_play_pause(&self.device.lock().unwrap())?,
			_ => {}
		}
		false
	}
}

#[throws] fn main() {
	let mut channel = dbus::channel::Channel::get_private(dbus::channel::BusType::Session)?;
	channel.set_watch_enabled(true);
	let session = dbus::blocking::LocalConnection::from(channel);
	session.request_name("org.mpris.MediaPlayer2.music", false, true, false)?;
	let mut dbus = dbus_crossroads::Crossroads::new();

	let output = audio::Output::new()?;
	let player = Arc::new(Player{device: Mutex::new(output.device), metadata: default()});
	let mut app = ui::app::App::new(&*player)?;
	let token = mpris::media_player2::register_org_mpris_media_player2_player(&mut dbus);
	dbus.insert("/org/mpris/MediaPlayer2", &[token], Arc::downgrade(&player));

	use futures_lite::FutureExt;
	async_io::block_on(
		async {
			loop {
				async_io::Async::new(session.channel().watch().fd)?.readable().await?;
				if let Some(msg) = session.channel().blocking_pop_message(std::time::Duration::from_millis(0))? {
					if msg.msg_type() == dbus::message::MessageType::MethodCall {
							dbus.handle_message(msg, &session).unwrap();
					} else if let Some(reply) = dbus::channel::default_reply(&msg) {
							let _ = session.channel().send(reply);
					}
        }
			}
		}
		.or(async { app.display().await; Ok(()) })
		.or(player.play(output.output, "."))
	)?
}
