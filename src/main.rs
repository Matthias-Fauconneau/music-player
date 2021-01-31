#![feature(default_free_fn, async_closure, generators, generator_trait, box_syntax)]
use {fehler::throws, anyhow::{Error, Result}, std::default::default};
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

struct ObjectServer(zbus::ObjectServer);
impl ObjectServer {
	const PATH: &'static str = "/org/mpris/MediaPlayer2";
	fn with<R>(&self, f: impl Fn(&Player)->R) -> zbus::Result<R> { self.0.with(Self::PATH, |o| Ok(f(o))) }
}

impl ui::widget::Widget for ObjectServer {
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

	let audio::Output{ref device, mut output} = audio::Output::new()?;
	let mut object_server = zbus::ObjectServer::new(&dbus);
	object_server.at(ObjectServer::PATH, Player{device, metadata: default()})?;
	let mut app = ui::app::App::new(ObjectServer(object_server))?;
	use futures_lite::stream::{self, StreamExt};
	app.streams.push(async_io::block_on(dbus.0.stream()).map(|message| (box move |app: &mut ui::app::App<ObjectServer>| { app.widget.0.dispatch_message(&message?)?; Ok(()) }) as Box<dyn FnOnce(&mut _)->Result<()>>).boxed_local());
	enum YieldAwait<Y, A> { Yield(Y), Await(A) }
	app.streams.push(stream::try_unfold(|| -> Result<_> {
		for entry in walkdir::WalkDir::new(".").into_iter().filter(|e| e.as_ref().unwrap().file_type().is_file()) {
			let (mut reader, metadata, mut decoder) = open(entry?.path())?;
			yield YieldAwait::Yield(metadata);
			while let Ok(packet) = reader.next_packet() {
				let buffer = decoder.decode(&packet)?;
				let write = {use symphonia::core::audio::AudioBufferRef::*; match buffer {
					S32(buffer) => Box::pin(write(device, &mut output, &buffer)) as std::pin::Pin<Box<dyn core::future::Future<Output=Result<()>>>>,
					F32(buffer) => Box::pin(write(device, &mut output, &buffer)) as std::pin::Pin<Box<dyn core::future::Future<Output=Result<()>>>>,
				}};
				yield YieldAwait::Await(write);
			}
		}
		Ok(())
	}, async move |mut generator| {
		let item = loop {
			use std::ops::Generator;
			match std::pin::Pin::new(&mut generator).resume(()) {
					std::ops::GeneratorState::Yielded(YieldAwait::Yield(y)) => { break Some(y); },
					std::ops::GeneratorState::Yielded(YieldAwait::Await(a)) => a.await?,
					std::ops::GeneratorState::Complete(r) => { r?; break None; },
			}
		};
		Ok(item.map(|item| (item, generator)))
	})
	.map(|metadata:Result<_>| (box move |app: &mut ui::app::App<ObjectServer>| {
		app.widget.with(|o| o.metadata = metadata.unwrap());
		app.draw()
	}) as Box<dyn FnOnce(&mut _)->Result<()>>).boxed_local());
	app.run()
}
