#![feature(default_free_fn, async_closure, box_syntax)]
use {fehler::throws, anyhow::{Error, Result}, std::{default::default, cell::RefCell}};
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

pub struct Player<'t> {
	device: &'t alsa::PCM,
	metadata: std::collections::HashMap<String, String>,
}
impl Player<'_> {
pub(self) const PATH: &'static str = "/org/mpris/MediaPlayer2";
}

struct ObjectServer<'t>(&'t RefCell<zbus::ObjectServer>);
impl ObjectServer<'_> {
	const PATH: &'static str = Player::PATH;
	fn with<R>(&self, f: impl FnOnce(&Player)->R) -> zbus::Result<R> { self.0.borrow().with(Self::PATH, move |o| Ok(f(o))) }
	fn with_mut<R>(&self, f: impl FnOnce(&mut Player)->R) -> zbus::Result<R> { self.0.borrow().with_mut(Self::PATH, move |o| Ok(f(o))) }
}

impl ui::widget::Widget for ObjectServer<'_> {
	#[throws] fn paint(&mut self, _: &mut ui::widget::Target) {
	}
	#[throws] fn event(&mut self, _: ui::widget::size, _: &ui::widget::EventContext, event: &ui::widget::Event) -> bool {
		match event {
			ui::widget::Event::Key{key:' '} => self.with(|o| toggle_play_pause(&o.device))??,
			_ => {}
		}
		false
	}
}

#[throws] fn main() {
	let dbus = zbus::Connection::new_session()?;
	zbus::fdo::DBusProxy::new(&dbus)?.request_name("org.mpris.MediaPlayer2.RustMusic", default())?;
	let audio::Output{ref device, output} = audio::Output::new()?;
	let ref object_server = RefCell::new(zbus::ObjectServer::new(&dbus));
	let mut playlist = walkdir::WalkDir::new(".").into_iter().filter(|e| e.as_ref().unwrap().file_type().is_file());
	let (reader, metadata, decoder) = open(playlist.next().unwrap()?.path())?;
	let _mpris = mpris::at(object_server, Player{device, metadata})?;
	let mut app = ui::app::App::new(ObjectServer(object_server))?;
	use futures_lite::stream::{self, StreamExt};
	app.streams.push(async_io::block_on(dbus.0.stream()).map(|message| (box move |app: &mut ui::app::App<ObjectServer>| { app.widget.0.borrow_mut().dispatch_message(&message?)?; Ok(()) }) as Box<dyn FnOnce(&mut _)->Result<()>>).boxed_local());
	app.streams.push(stream::try_unfold((output, playlist, (reader, decoder)), async move |(mut output, mut playlist, (mut reader, mut decoder))| {
		while let Ok(packet) = reader.next_packet() {
			{use symphonia::core::audio::AudioBufferRef::*; match decoder.decode(&packet)? {
				S32(buffer) => write(device, &mut output, &buffer).await?,
				F32(buffer) => write(device, &mut output, &buffer).await?,
			}};
		}
		playlist.next().map(|entry| {
			let (reader, metadata, decoder) = open(entry?.path())?;
			Ok((metadata, (output, playlist, (reader, decoder))))
		}).transpose()
	})
	.map(|metadata:Result<_>| (box move |app: &mut ui::app::App<ObjectServer>| {
		app.widget.with_mut(|o| o.metadata = metadata.unwrap())?;
		app.draw()
	}) as Box<dyn FnOnce(&mut _)->Result<()>>).boxed_local());
	app.run()?
}
