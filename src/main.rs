#![feature(default_free_fn, async_closure, box_syntax, try_blocks)]
pub fn take<T>(t: &mut T, f: impl FnOnce(T) -> T) {unsafe {std::ptr::write(t, std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(std::ptr::read(t)))).unwrap_or_else(|_| ::std::process::abort()))}}
use {fehler::throws, anyhow::{Error, Result, Context}, std::{default::default, cell::RefCell}};
mod audio;
mod mpris;

use symphonia::core::{*, audio::AudioBuffer};
#[throws] async fn write<S: sample::Sample>(output: &RefCell<audio::Output>, buffer: &AudioBuffer<S>) where i16: conv::FromSample<S> {
	let rate = buffer.spec().rate;
	if output.borrow().device.hw_params_current()?.get_rate()? != rate { take(&mut *output.borrow_mut(), |o|{ drop(o); audio::Output::new(rate).unwrap() }); }
	use symphonia::core::audio::Signal;
	let mut samples = buffer.chan(0).into_iter().map(|&v| conv::FromSample::from_sample(v)).zip(buffer.chan(1).into_iter().map(|&v| conv::FromSample::from_sample(v)));
	while samples.len() > 0 { output.borrow().write(&mut samples).await?; }
}

#[throws] fn open(path: &std::path::Path) -> (Box<dyn formats::FormatReader>, std::collections::HashMap<String, String>, Box<dyn codecs::Decoder>) {
	let file = symphonia::default::get_probe().format(&symphonia::core::probe::Hint::new().with_extension(path.extension().unwrap().to_str().unwrap()),
																																									 symphonia::core::io::MediaSourceStream::new(Box::new(std::fs::File::open(path)?)), &default(), &default())?;
	let container = file.format;
	let ref codec_parameters = container.default_stream().unwrap().codec_params;
	let mut metadata = std::collections::HashMap::new();
	for tag in container.metadata().current().or(file.metadata.current()).unwrap().tags() { if let Some(key) = tag.std_key {
		let key = {use meta::StandardTagKey::*; match key {
			Artist|OriginalArtist|SortArtist|Lyricist|Arranger => "xesam:artist",
			TrackNumber => "xesam:TrackNumber",
			Date|OriginalDate => "xesam:contentCreated",
			Genre => "xesam:genre",
			Bpm => "xesam:audioBPM",
			Album|DiscSubtitle => "xesam:album",
			AlbumArtist|SortAlbumArtist => "xesam:albumArtist",
			TrackTitle => "xesam:title",
			Composer|SortComposer => "xesam:composer",
			MediaFormat|Language|Lyrics|Label|IdentIsrc|Writer|Url|Comment|Copyright|Encoder|EncodedBy|TrackTotal|Script|IdentCatalogNumber|Description|ReleaseCountry|DiscNumber|DiscTotal
			|MusicBrainzAlbumArtistId|MusicBrainzAlbumId|MusicBrainzArtistId|MusicBrainzReleaseGroupId|MusicBrainzReleaseTrackId|MusicBrainzTrackId=> "",
			key => {println!("{:?}", key); ""},
		}};
		if !key.is_empty() { metadata.insert(key.to_string(), tag.value.clone()); }
	}}
	let path = path.canonicalize()?;
	metadata.insert("xesam:url".to_string(), format!("file://{}", path.to_str().unwrap()));
	metadata.insert("mpris:artUrl".to_string(), format!("file://{}", path.with_file_name("cover.jpg").to_str().unwrap()));
	let decoder = symphonia::default::get_codecs().make(&codec_parameters, &default())?;
	(container, metadata, decoder)
}

pub struct Player<'t> {
	audio: &'t RefCell<audio::Output>,
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
	#[throws] fn paint(&mut self, target: &mut ui::widget::Target) {
		target.fill(0.into());
		let path = self.with(|o| url::Url::parse(o.metadata.get("mpris:artUrl").unwrap()))??;
		let path = path.to_file_path().map_err(|_| Error::msg(path))?;
		let _ : Result<()> = try {
			let image = image_io::io::Reader::open(path)?.decode()?.into_rgb8();
			let image = image::Image::<&[image::rgb::rgb::<u8>]>::new(image.dimensions().into(), unsafe{image::slice::cast(&image)});
			let size = ui::text::fit(target.size, image.size);
			let mut target = target.slice_mut((target.size-size)/2, size);
			let size = target.size;
			target.set(|p| image[p*(image.size-1.into())/(size-1.into())].into());
		};
		if !self.with(|o| o.audio.borrow().playing())? {
			let size = std::cmp::min(target.size.x, target.size.y).into();
			let mut target = target.slice_mut((target.size-size)/2, size);
			use xy::xy;
			image::invert(&mut target.slice_mut(size*xy{x:1, y:1}/xy{x:5, y:5}, size*xy{x:1, y:3}/xy{x:5, y:5}), true.into());
			image::invert(&mut target.slice_mut(size*xy{x:3, y:1}/xy{x:5, y:5}, size*xy{x:1, y:3}/xy{x:5, y:5}), true.into());
		}
	}
	#[throws] fn event(&mut self, _: ui::widget::size, _: &ui::widget::EventContext, event: &ui::widget::Event) -> bool {
		match event {
			ui::widget::Event::Key{key:' '} => {self.with(|o| o.audio.borrow().toggle_play_pause())??; true},
			_ => false
		}
	}
}

#[throws] fn main() {
	let dbus = zbus::Connection::new_session()?;
	zbus::fdo::DBusProxy::new(&dbus)?.request_name("org.mpris.MediaPlayer2.RustMusic", default())?;
	let ref audio= RefCell::new(audio::Output::new(48000)?);
	let ref object_server = RefCell::new(zbus::ObjectServer::new(&dbus));
	let playlist = walkdir::WalkDir::new(".").into_iter().filter(|e| e.as_ref().unwrap().file_type().is_file()).filter_map(|e| e.ok()).collect::<Box<_>>();
	let mut playlist = std::iter::from_fn(move || loop {
		use rand::seq::SliceRandom;
		if let Some(entry) = playlist.choose(&mut rand::thread_rng()) {
			let path = entry.path();
			println!("{}", path.display());
			if let Ok(next) = open(path).context(path.to_str().unwrap().to_owned()) {
				break Some(next);
			}
		} else { break None; }
	});
	let (reader, metadata, decoder) = playlist.next().unwrap();
	let _mpris = mpris::at(object_server, Player{audio, metadata})?;
	let mut app = ui::app::App::new(ObjectServer(object_server))?;
	use futures_lite::stream::{self, StreamExt};
	app.streams.push(async_io::block_on(dbus.0.stream()).map(|message| (box move |app: &mut ui::app::App<ObjectServer>| {
		app.widget.0.borrow_mut().dispatch_message(&message?)?;
		app.draw()
	}) as Box<dyn FnOnce(&mut _)->Result<()>>).boxed_local());
	app.streams.push(stream::try_unfold((audio, playlist, (reader, decoder)), async move |(audio, mut playlist, (mut reader, mut decoder))| {
		while let Ok(packet) = reader.next_packet() {
			{use symphonia::core::audio::AudioBufferRef::*; match decoder.decode(&packet)? {
				S32(buffer) => write(audio, &buffer).await?,
				F32(buffer) => write(audio, &buffer).await?,
			}};
		}
		let next = if let Some((reader, metadata, decoder)) = playlist.next() { Some((metadata, (audio, playlist, (reader, decoder)))) } else { None };
		Ok(next)
	})
	.map(|metadata:Result<_>| (box move |app: &mut ui::app::App<ObjectServer>| {
		app.widget.with_mut(|o| o.metadata = metadata.unwrap())?;
		app.draw()
	}) as Box<dyn FnOnce(&mut _)->Result<()>>).boxed_local());
	app.run()?
}
