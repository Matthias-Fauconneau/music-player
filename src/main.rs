#![feature(default_free_fn, try_blocks, exact_size_is_empty)]
mod audio;
mod mpris;

use {ui::*, std::default::default, symphonia::core::{formats, codecs, meta}};
fn open(path: &std::path::Path) -> Result<(Box<dyn formats::FormatReader>, std::collections::HashMap<String, String>, Box<dyn codecs::Decoder>)> {
	let mut file = symphonia::default::get_probe().format(&symphonia::core::probe::Hint::new().with_extension(path.extension().ok_or("")?.to_str().unwrap()),
																																									 symphonia::core::io::MediaSourceStream::new(Box::new(std::fs::File::open(path)?), default()), &default(), &default())?;
	let mut container = file.format;
	let mut metadata = std::collections::HashMap::<String, String>::new();
	for tag in container.metadata().current().or(file.metadata.get().unwrap().current()).unwrap().tags() { if let Some(key) = tag.std_key {
		let key = {use meta::StandardTagKey::*; match key {
			Artist|OriginalArtist|SortArtist|Lyricist|Arranger => "xesam:artist",
			TrackNumber => "xesam:TrackNumber",
			Date|OriginalDate => "xesam:contentCreated",
			Genre => "xesam:genre",
			Bpm => "xesam:audioBPM",
			Album|DiscSubtitle => "xesam:album",
			AlbumArtist|SortAlbumArtist => "xesam:albumArtist",
			TrackTitle|TrackSubtitle => "xesam:title",
			Composer|SortComposer => "xesam:composer",
			MediaFormat|Language|Lyrics|Label|IdentIsrc|Writer|Url|Comment|Copyright|Encoder|EncodedBy|TrackTotal|Script|IdentCatalogNumber|Description|ReleaseCountry|DiscNumber|DiscTotal|Ensemble|Rating
			|MusicBrainzAlbumArtistId|MusicBrainzAlbumId|MusicBrainzArtistId|MusicBrainzReleaseGroupId|MusicBrainzReleaseTrackId|MusicBrainzTrackId=> "",
			key => {println!("{:?}", key); ""},
		}};
		if !key.is_empty() { metadata.insert(key.to_string(), tag.value.to_string()); }
	}}
	let path = path.canonicalize()?;
	metadata.insert("xesam:url".to_string(), format!("file://{}", path.to_str().unwrap()));
	metadata.insert("mpris:artUrl".to_string(), format!("file://{}", path.with_file_name("cover.jpg").to_str().unwrap()));
	let decoder = symphonia::default::get_codecs().make(&container.default_track().unwrap().codec_params, &default())?;
	Ok((container, metadata, decoder))
}

#[derive(Default)] pub struct Player {
	audio: audio::Output,
	metadata: std::collections::HashMap<String, String>,
}

use std::sync::{Arc,Mutex};
#[derive(Default,Clone)] struct Arch<T>(Arc<Mutex<T>>);
impl<T> Arch<T> {
    //pub fn new(inner: T) -> Self { Self(std::sync::Arc::new(std::sync::Mutex::new(inner))) }
	pub fn clone(&self) -> Self { Self(self.0.clone()) }
    pub fn lock(&self) -> std::sync::MutexGuard<T> {self.0.lock().unwrap() }
}
unsafe impl<T> Send for Arch<T> {}
unsafe impl<T> Sync for Arch<T> {}
impl<T:Widget> Widget for Arch<T> {
	fn paint(&mut self, target: &mut Target, size: size, offset: int2) -> Result { self.0.lock().unwrap().paint(target, size, offset) }
	fn event(&mut self, size: size, context: &mut EventContext, event: &Event) -> Result<bool> { self.0.lock().unwrap().event(size, context, event) }
}

impl Widget for Player {
	#[throws] fn paint(&mut self, target: &mut Target, _: size, _: int2) {
		target.fill(background.into());
		let path = url::Url::parse(self.metadata.get("mpris:artUrl").ok_or("Nothing to play")?)?;
		let path = path.to_file_path().map_err(|_| anyhow::Error::msg(path))?;
		let _ : Result<()> = try {
			let image = image_io::io::Reader::open(path)?.decode()?.into_rgb8();
			let image = image::Image::<&[image::rgb::<u8>]>::cast_slice(&image, image.dimensions().into());
			let size = ui::text::fit(target.size, image.size);
			let mut target = target.slice_mut((target.size-size)/2, size);
			let size = target.size;
			target.set(|p| u32::from(image[p*(image.size-size::from(1))/(size-size::from(1))]));
		};
		if !self.audio.playing() {
			let size = std::cmp::min(target.size.x, target.size.y).into();
			let mut target = target.slice_mut((target.size-size)/2, size);
			use image::xy;
			image::invert(&mut target.slice_mut(size*xy{x:1, y:1}/xy{x:5, y:5}, size*xy{x:1, y:3}/xy{x:5, y:5}), true.into());
			image::invert(&mut target.slice_mut(size*xy{x:3, y:1}/xy{x:5, y:5}, size*xy{x:1, y:3}/xy{x:5, y:5}), true.into());
		}
	}
	#[throws] fn event(&mut self, _: size, _: &mut EventContext, event: &Event) -> bool {
		match event {
			Event::Key(' ') => { self.audio.toggle_play_pause()?; true },
			_ => false
		}
	}
}


#[async_std::main]
async fn main() -> Result {
	let mut player : Arch<Player> = default();
	let _mpris = zbus::ConnectionBuilder::session()?.name("org.mpris.MediaPlayer2.RustMusic")?.serve_at("/org/zbus/RustMusic", Arch::clone(&player))?.build().await?;
	std::thread::spawn({let player : Arch<Player> = Arch::clone(&player); move || {
		let playlist = walkdir::WalkDir::new(".").into_iter().filter(|e| e.as_ref().unwrap().file_type().is_file()).filter_map(|e| e.ok()).collect::<Box<_>>();
		let playlist = std::iter::from_fn(move || loop {
			use rand::seq::SliceRandom;
			if let Some(entry) = playlist.choose(&mut rand::thread_rng()) {
				let path = entry.path();
				println!("{}", path.display());
				if let Ok(next) = open(path) { break Some(next); }
			} else { break None; }
		});
		for (mut reader, metadata, mut decoder) in playlist {
			player.lock().metadata = metadata; // TODO: eventfd channel to UI poll to trigger UI update on metadata change
			Result::<()>::unwrap(try {
				while let Ok(packet) = reader.next_packet() {
					use symphonia::core::{audio::AudioBuffer, sample, conv};
					#[throws] fn write<S: sample::Sample>(output: &mut audio::Output, buffer: &AudioBuffer<S>) where i16: conv::FromSample<S> {
						let rate = buffer.spec().rate;
						if output.device.hw_params_current()?.get_rate()? != rate {
							while let Err(err) = output.device.drain() { if let alsa::nix::errno::Errno::EAGAIN = err.errno() { continue; } else { fehler::throw!(err); } }
							*output = audio::Output::new(rate).unwrap();
						}
						use symphonia::core::audio::Signal;
						let mut samples = buffer.chan(0).into_iter().map(|&v| conv::FromSample::from_sample(v)).zip(buffer.chan(1).into_iter().map(|&v| conv::FromSample::from_sample(v)));
						while !samples.is_empty() { output.write(&mut samples)?; } // TODO: fade out and return on UI quit
					}
					let audio = &mut player.lock().audio;
					use symphonia::core::audio::AudioBufferRef::*; match decoder.decode(&packet)? {
						S32(buffer) => write(audio, &buffer)?,
						F32(buffer) => write(audio, &buffer)?,
						_ => unimplemented!(),
					};
				}
			});
		}
	}});
	run(&mut player)
}
