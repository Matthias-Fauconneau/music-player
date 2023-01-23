#![feature(default_free_fn, try_blocks, exact_size_is_empty, trait_alias, let_chains, associated_type_bounds, array_zip, array_methods, anonymous_lifetime_in_impl_trait)]
#![allow(mixed_script_confusables)]
mod audio;
mod mpris;
mod resampler;

use {ui::*, std::default::default, symphonia::core::{formats, codecs, meta}};
fn open(path: &std::path::Path) -> Result<(Box<dyn formats::FormatReader>, std::collections::HashMap<String, String>, Box<dyn codecs::Decoder>)> {
	let mut file = symphonia::default::get_probe().format(&symphonia::core::probe::Hint::new().with_extension(path.extension().ok_or("")?.to_str().unwrap()),
																																									 symphonia::core::io::MediaSourceStream::new(Box::new(std::fs::File::open(path)?), default()), &default(), &default())?;
	let mut container = file.format;
	let file_metadata = file.metadata.get();
	let file_metadata = file_metadata.as_ref();
	let mut metadata = std::collections::HashMap::<String, String>::new();
	for tag in container.metadata().current().or_else(|| file_metadata.unwrap().current() ).unwrap().tags() { if let Some(key) = tag.std_key {
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
			|MusicBrainzAlbumArtistId|MusicBrainzAlbumId|MusicBrainzArtistId|MusicBrainzReleaseGroupId|MusicBrainzReleaseTrackId|MusicBrainzTrackId|EncoderSettings|ReplayGainTrackGain|ReplayGainTrackPeak|ReplayGainAlbumGain|ReplayGainAlbumPeak=> "",
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

use audio::{Output as Audio, Write as _};
#[derive(Default)] pub struct Player {
	audio: Audio,
	metadata: std::collections::HashMap<String, String>,
}

use {std::sync::Arc, parking_lot::{Mutex, MutexGuard}};
#[derive(Default,Clone)] struct Arch<T>(Arc<Mutex<T>>);
impl<T> Arch<T> {
    //pub fn new(inner: T) -> Self { Self(std::sync::Arc::new(std::sync::Mutex::new(inner))) }
	pub fn clone(&self) -> Self { Self(self.0.clone()) }
    pub fn lock(&self) -> MutexGuard<T> { self.0.lock() }
}
unsafe impl<T> Send for Arch<T> {}
unsafe impl<T> Sync for Arch<T> {}
impl<T:Widget> Widget for Arch<T> {
	fn paint(&mut self, target: &mut Target, size: size, offset: int2) -> Result { self.lock().paint(target, size, offset) }
	fn event(&mut self, size: size, context: &mut EventContext, event: &Event) -> Result<bool> { self.lock().event(size, context, event) }
}

impl Widget for Player {
	#[throws] fn paint(&mut self, target: &mut Target, _: size, _: int2) {
		target.fill(background.into());
		let _ : Result<()> = try {
			let path = url::Url::parse(self.metadata.get("mpris:artUrl").ok_or("No")?)?;
			let path = path.to_file_path().expect("Expecting local cover");
			let image = image_io::io::Reader::open(path)?.decode()?.into_rgb8();
			let image = image::Image::<&[image::rgb::<u8>]>::cast_slice(&image, image.dimensions().into());
			let size = ui::text::fit(target.size, image.size);
			let mut target = target.slice_mut((target.size-size)/2, size);
			let size = target.size;
			target.set(|p| image[p*(image.size-size::from(1))/(size-size::from(1))].g as u32);
			//target.set(|p| u32::from(image[p*(image.size-size::from(1))/(size-size::from(1))]));
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

//#[async_std::main]
/*async*/ fn main() -> Result {
	let mut player : Arch<Player> = default();
	let _mpris = zbus::ConnectionBuilder::session()?.name("org.mpris.MediaPlayer2.RustMusic")?.serve_at("/org/zbus/RustMusic", Arch::clone(&player))?.internal_executor(false).build(); //.await? TODO: poll
	std::thread::spawn({let player : Arch<Player> = Arch::clone(&player); move || Result::<()>::unwrap(try {
		let playlist = walkdir::WalkDir::new(std::env::args().skip(1).next().map(|s| s.into()).unwrap_or(xdg_user::music()?.unwrap_or_default())).into_iter().filter(|e| e.as_ref().unwrap().file_type().is_file()).filter_map(|e| e.ok()).collect::<Box<_>>();
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
			type Resampler = resampler::MultiResampler;
			let ref mut resampler = Resampler::new(decoder.codec_params().sample_rate.unwrap(), player.lock().audio.device.hw_params_current()?.get_rate()?);
			use symphonia::core::{sample, conv, formats::Packet};
			//trait Decoder { fn decode(&mut self, _: &Packet) -> AudioBufferRef; }
			#[throws(alsa::Error)] fn write<MutexGuard: std::ops::Deref<Target=Audio>, S: sample::Sample+'static, T>(
				ref audio: impl Fn() -> MutexGuard,
				resampler: &mut Option<Resampler>,
				//packets: impl Iterator<Item=Buffer<S>>
				ref mut packets: impl Iterator<Item=Packet>,
				ref mut decode: impl FnMut(&Packet) -> T,
				ref upcast: impl Fn(&T) -> std::borrow::Cow<'_, Buffer<S>>,
				//packets: impl Iterator<Item=[impl ExactSizeIterator<Item=S>; 2]>)
				) where f32: conv::FromSample<S>, i16: conv::FromSample<S> {

				/*fn from_sample<S,T>(packets: impl Iterator<Item=[impl ExactSizeIterator<Item=S>; 2]>) -> impl Iterator<Item=[impl ExactSizeIterator<Item=T>; 2]>
				where T: conv::FromSample<S> { packets.map(|packet| packet.map(|channel| channel.map(|v| conv::FromSample::from_sample(v)))) }*/

				fn convert<'b: 'bo, 'bo: 'i, 'i, S:Sample,T>(packet: &'bo std::borrow::Cow<'b, Buffer<S>>) -> [impl ExactSizeIterator<Item=T>+'i; 2]
				where T: conv::FromSample<S> { [0,1].map(|channel| packet.chan(channel).iter().map(|&v| conv::FromSample::from_sample(v))) }

				/*fn from_sample<'t, S:Sample+'t,T>(packets: impl Iterator<Item=&'t Buffer<S>>) -> impl Iterator<Item=[impl ExactSizeIterator<Item=T>+'t; 2]>
				where T: conv::FromSample<S> { packets.map(|packet| [0,1].map(|channel| packet.chan(channel).iter().map(|&v| conv::FromSample::from_sample(v)))) }*/

				if let Some(resampler) = resampler {
					while let Some([L, R]) = resampler.resample(packets, &mut *decode, upcast, convert) {
						let f32_to_i16 = |s| { (f32::clamp(s, -1., 1.) * 32768.) as i16 };
						audio.write(L.map(f32_to_i16).zip(R.map(f32_to_i16)))?;
					}
				} else {
					for packet in packets { let buffer = decode(&packet); let buffer = upcast(&buffer); let [L, R] = convert(&buffer); audio.write(L.zip(R))?; }
				}
			}
			let audio = || MutexGuard::map(player.lock(), |unlocked_player| &mut unlocked_player.audio);
			let packets = std::iter::from_fn(|| reader.next_packet().ok());
			let sample_format = decoder.codec_params().sample_format.unwrap();
			let decode = move |packet| decoder.decode(packet).unwrap();
			/*impl FnMut(Packet) -> T for Decoder {
				decoder.decode(&packet).unwrap();
			}*/
			//let packets = std::iter::from_fn(|| reader.next_packet().map(|packet| decoder.decode(&packet).unwrap()).ok());
			use symphonia::core::{sample::Sample, audio::{AudioBuffer as Buffer, Signal as _}, sample::SampleFormat};
			//fn iter<'t, S:Sample+'t>(buffer: &'t Buffer<S>) -> [impl ExactSizeIterator<Item=S>+'t; 2] { [0,1].map(|channel| buffer.chan(channel).into_iter().copied()) }
			// TODO: fade out and return on UI quit
			use symphonia::core::audio::AudioBufferRef;
			match sample_format {
				SampleFormat::S32 => write(audio, resampler, packets, decode, |buffer| if let AudioBufferRef::S32(b) = buffer { std::borrow::Cow::Borrowed(b) } else { unreachable!() }),
				SampleFormat::F32 => write(audio, resampler, packets, decode, |buffer| if let AudioBufferRef::F32(b) = buffer { std::borrow::Cow::Borrowed(b) } else { unreachable!() }),
				/*SampleFormat::S32 => write(audio, resampler, packets.map(|packet| if let AudioBufferRef::S32(p) = packet { iter(p) } else { unreachable!() })),
				SampleFormat::F32 => write(audio, resampler, packets.map(|packet| if let AudioBufferRef::F32(p) = packet { iter(p) } else { unreachable!() })),*/
				_ => unimplemented!(),
			}?
		}
	})});
	run("Player", &mut player)
}
