#![feature(default_free_fn, in_band_lifetimes, never_type)]
use {fehler::throws, anyhow::Error, std::{default::default, cell::RefCell}};
mod audio;
mod mpris;

fn playing(device: &alsa::PCM) -> bool { device.state() == alsa::pcm::State::Running }
fn toggle_play_pause(device: &alsa::PCM) -> alsa::Result<()> { device.pause(playing(&device)) }

use symphonia::core::{*, audio::AudioBuffer};
#[throws] async fn write<S: sample::Sample>(device: &alsa::PCM, mut output: &mut alsa::direct::pcm::MmapPlayback<i16>, buffer: &AudioBuffer<S>) where i16: conv::FromSample<S> {
	assert_eq!(buffer.spec().rate, device.hw_params_current()?.get_rate()?);
	use symphonia::core::audio::Signal;
	let mut samples = buffer.chan(0).into_iter().map(|&v| conv::FromSample::from_sample(v)).zip(buffer.chan(1).into_iter().map(|&v| conv::FromSample::from_sample(v)));
	while samples.len() > 0 {
		audio::write(device, &mut output, &mut samples)?;
		async_io::Async::new(alsa::PollDescriptors::get(device)?[0].fd)?.writable().await?;
	}
}

//*self.metadata.lock().unwrap() = metadata;
#[throws] fn open(path: &std::path::Path) -> (Box<dyn formats::FormatReader>, std::collections::HashMap<String, String>, Box<dyn codecs::Decoder>) {
	let file = symphonia::default::get_probe().format(&symphonia::core::probe::Hint::new().with_extension(path.extension().unwrap().to_str().unwrap()),
																																									 symphonia::core::io::MediaSourceStream::new(Box::new(std::fs::File::open(path)?)), &default(), &default())?;
	let container = file.format;
	let ref codec_parameters = container.default_stream().unwrap().codec_params;
	let mut metadata = std::collections::HashMap::new();
	for tag in container.metadata().current().or(file.metadata.current()).unwrap().tags() { if let Some(key) = tag.std_key {
		let key = {use meta::StandardTagKey::*; match key {
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
	metadata.insert("xesam:url".to_string(), format!("file://{}", path.to_str().unwrap()));
	metadata.insert("mpris:artUrl".to_string(), format!("file://{}", path.with_file_name("cover.jpg").to_str().unwrap()));
	let decoder = symphonia::default::get_codecs().make(&codec_parameters, &default())?;
	(container, metadata, decoder)
}

pub struct Player {
	device: alsa::PCM,
	metadata: std::cell::RefCell<std::collections::HashMap<String, String>>,
}

impl ui::widget::Widget for &Player {
	#[throws] fn paint(&mut self, _: &mut ui::widget::Target) {
	}
	#[throws] fn event(&mut self, _: ui::widget::size, _: &ui::widget::EventContext, event: &ui::widget::Event) -> bool {
		match event {
			ui::widget::Event::Key{key:' '} => toggle_play_pause(&self.device)?,
			_ => {}
		}
		false
	}
}

async fn dbus(dbus: zbus::Connection, object_server: &RefCell<zbus::ObjectServer>) -> zbus::Result<()> {
	loop {
		let msg = dbus.0.receive_specific(|_| Ok(true)).await?;
		object_server.borrow_mut().dispatch_message(&msg)?;
	}
}

#[throws] fn main() {
	let dbus = zbus::Connection::new_session()?;
	zbus::fdo::DBusProxy::new(&dbus)?.request_name("org.mpris.MediaPlayer2.RustMusic", default())?;

	let audio::Output{device, mut output} = audio::Output::new()?;
	let ref player = Player{device, metadata: default()};
	let ref device = player.device;
	let ref object_server = RefCell::new(zbus::ObjectServer::new(&dbus));
	let _mpris = mpris::at(object_server, player)?;
	let mut app = ui::app::App::new(player)?;

	use futures_lite::FutureExt;
	use futures_util::future::TryFutureExt;
	async_io::block_on(
		self::dbus(dbus, object_server).map_err(Error::new)
		.or(async { app.display().await; Ok(()) })
		.or(async {
			for entry in walkdir::WalkDir::new(".").into_iter().filter(|e| e.as_ref().unwrap().file_type().is_file()) {
				let (mut reader, metadata, mut decoder) = open(entry?.path())?;
				player.metadata.replace(metadata);
				while let Ok(packet) = reader.next_packet() {
					{use symphonia::core::audio::AudioBufferRef::*; match decoder.decode(&packet)? {
						S32(buffer) => write(device, &mut output, &buffer).await,
						F32(buffer) => write(device, &mut output, &buffer).await,
					}}?;
				}
			}
			Ok(())
		})
	)?
}
