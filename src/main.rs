#![feature(exact_size_is_empty, impl_trait_in_assoc_type, let_chains)]#![allow(mixed_script_confusables)]
mod audio;
#[cfg(feature="zbus")] mod mpris;
mod resampler;
fn default<T: Default>() -> T { Default::default() }
type Result<T = (), E = Box<dyn std::error::Error>>  = std::result::Result<T, E>;

use symphonia::core::{formats, codecs, meta};
#[cfg(feature="ui")] use ui::*;

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
			|MusicBrainzAlbumArtistId|MusicBrainzAlbumId|MusicBrainzArtistId|MusicBrainzReleaseGroupId|MusicBrainzReleaseTrackId|MusicBrainzTrackId|EncoderSettings|ReplayGainTrackGain|ReplayGainTrackPeak|ReplayGainAlbumGain|ReplayGainAlbumPeak|Performer=> "",
			key => {println!("{:?}", key); ""},
		}};
		if !key.is_empty() { metadata.insert(key.to_string(), tag.value.to_string()); }
	}}
	let path = path.canonicalize()?;
	metadata.insert("xesam:url".to_string(), format!("file://{}", path.to_str().unwrap()));
	if let Some(art) = ["png","jpg"].iter().find_map(|ext| { let cover = path.with_file_name(format!("cover.{ext}")); cover.exists().then(|| cover) }) {
		metadata.insert("mpris:artUrl".to_string(), format!("file://{}", art.to_str().unwrap()));
	}
	let decoder = symphonia::default::get_codecs().make(&container.default_track().unwrap().codec_params, &default())?;
	Ok((container, metadata, decoder))
}

use audio::{PCM, Write as _};
#[derive(Default)] pub struct Player {
	output: Vec<PCM>,
	metadata: std::collections::HashMap<String, String>,
}
impl Player {
	fn new(output: &[&str]) -> Self { Self{output: Vec::from_iter(output.into_iter().map(|path| PCM::new(path, 48000).unwrap())), ..default()} }
	//fn title(&self) -> Result<&String> { self.metadata.get("xesam:title").ok_or("Missing title".into()) }
}


use {std::sync::Arc, parking_lot::{Mutex, MutexGuard}};
#[derive(Default,Clone)] struct Arch<T>(Arc<Mutex<T>>);
impl<T> Arch<T> {
    pub fn new(inner: T) -> Self { Self(std::sync::Arc::new(Mutex::new(inner))) }
	//pub fn clone(&self) -> Self { Self(self.0.clone()) }
    pub fn lock(&self) -> MutexGuard<T> { self.0.lock() }
}
unsafe impl<T> Send for Arch<T> {}
unsafe impl<T> Sync for Arch<T> {}

fn main() -> Result {
	const N: usize = 2;
	let player : Arch<Player> = Arch::new(Player::new(if N == 1 {&["/dev/snd/pcmC0D0p"]} else {&["/dev/snd/pcmC0D2p","/dev/snd/pcmC0D0p"]}));
	let root = std::env::args().skip(1).next().map(std::path::PathBuf::from);
	//let root = root.or(xdg_user::music()?);
	let playlist = walkdir::WalkDir::new(root.unwrap_or(std::env::current_dir()?)).follow_links(true).into_iter().filter(|e| e.as_ref().unwrap().file_type().is_file()).filter_map(|e| e.ok()).collect::<Box<_>>();
	let playlist = {let mut playlist = playlist; rand::seq::SliceRandom::shuffle(&mut *playlist, &mut rand::thread_rng()); playlist};
	let playlist = playlist.into_iter().filter_map(|entry| {
		let path = entry.path();
		println!("{}", path.display());
		open(path).ok()
	});
	/*let playlist = std::iter::from_fn(move || loop {
		use rand::seq::SliceRandom;
		if let Some(entry) = playlist.choose(&mut rand::thread_rng()) {
			let path = entry.path();
			println!("{}", path.display());
			if let Ok(next) = open(path) { break Some(next); }
		} else { break None; }
	});*/
	for (mut reader, metadata, mut decoder) in playlist {
		player.lock().metadata = metadata;
		//app.trigger()?;
		type Resampler = resampler::MultiResampler;
		let ref mut resampler = Resampler::new(decoder.codec_params().sample_rate.unwrap(), player.lock().output[0].rate);
		use {std::borrow::Cow, symphonia::core::{formats::Packet, audio::{AudioBufferRef, AudioBuffer, Signal as _}, sample::{self, Sample, SampleFormat}, conv}};
		trait Cast<'t, S:Sample> { fn cast(self) -> Cow<'t, AudioBuffer<S>>; }
		impl<'t> Cast<'t, i32> for AudioBufferRef<'t> { fn cast(self) -> Cow<'t, AudioBuffer<i32>> { if let AudioBufferRef::S32(variant) = self  { variant } else { unreachable!() } } }
		impl<'t> Cast<'t, f32> for AudioBufferRef<'t> { fn cast(self) -> Cow<'t, AudioBuffer<f32>> { if let AudioBufferRef::F32(variant) = self  { variant } else { unreachable!() } } }
		use resampler::SplitConvert;
		impl<S:Sample, T:conv::FromSample<S>> SplitConvert<T> for std::borrow::Cow<'_, AudioBuffer<S>> {
			type Channel<'t> = impl ExactSizeIterator<Item=T>+'t where Self: 't;
			fn split_convert<'t>(&'t self) -> [Self::Channel<'t>; 2]  { [0,1].map(move |channel| self.chan(channel).iter().map(|&v| conv::FromSample::from_sample(v))) }
		}
		struct Decoder<D,S>(D, std::marker::PhantomData<S>);
		impl<S:Sample+'static> resampler::Decoder<Packet> for Decoder<Box<dyn codecs::Decoder>, S> where
			for<'t> AudioBufferRef<'t>: Cast<'t, S> {
			type Buffer<'t> = Cow<'t, AudioBuffer<S>> where Self: 't;
			fn decode(&mut self, packet: &Packet) -> Self::Buffer<'_> { self.0.decode(packet).unwrap().cast() }
		}
		fn write
		<S: sample::Sample+'static, D, Output: std::ops::DerefMut<Target=[self::PCM; N]>, const N: usize>
		(resampler: &mut Option<Resampler>, ref mut packets: impl Iterator<Item=Packet>, decoder: D, ref mut output: impl FnMut() -> Output) -> audio::Result
			where Decoder<D, S>: resampler::Decoder<Packet>,
			for <'t> <Decoder<D, S> as resampler::Decoder<Packet>>::Buffer<'t>: SplitConvert<f32>,
			for <'t> <Decoder<D, S> as resampler::Decoder<Packet>>::Buffer<'t>: SplitConvert<i16> {
				#![allow(non_snake_case)]
			if let Some(resampler) = resampler.as_mut() {
				let mut decoder = Decoder(decoder, std::marker::PhantomData);
				while let Some([L, R]) = resampler.resample(packets, &mut decoder) {
					let f32_to_i16 = |s| f32::clamp(s*32768., -32768., 32767.) as i16;
					output.write(L.zip(R).map(|(L,R)| [L,R]).map(|[L,R]|[L,R].map(f32_to_i16)))?;
				}
			} else {
				let mut decoder = Decoder(decoder, std::marker::PhantomData);
				for ref packet in packets {
					let ref buffer = resampler::Decoder::decode(&mut decoder, packet);
					let [L, R] = SplitConvert::<i16>::split_convert(buffer);
					output.write(L.zip(R).map(|(L,R)| [L,R]))?;
				}
			}
			Ok(())
		}
		let output = || MutexGuard::map(player.lock(), |unlocked_player| <&mut [PCM; N]>::try_from(unlocked_player.output.as_mut_slice()).unwrap());
		let stop = false;
		let mut packets = std::iter::from_fn(|| (!stop).then(|| reader.next_packet().ok()).flatten()); // TODO: fade out
		let sample_format = decoder.codec_params().sample_format.unwrap_or_else(|| match decoder.decode(&packets.next().unwrap()).unwrap() {
			AudioBufferRef::S32(_) => SampleFormat::S32,
			AudioBufferRef::F32(_) => SampleFormat::F32,
			_ => unimplemented!(),
		});
		match sample_format {
			SampleFormat::S32 => write::<i32, _, _, N>(resampler, packets, decoder, output),
			SampleFormat::F32 => write::<f32, _, _, N>(resampler, packets, decoder, output),
			_ => unimplemented!(),
		}?;
		if stop { break; }
	}
	Ok(())
}

#[cfg(feature="ui")] impl<T:Widget> Widget for Arch<T> {
	fn paint(&mut self, target: &mut Target, size: size, offset: int2) -> Result { self.lock().paint(target, size, offset) }
	fn event(&mut self, size: size, context: &mut EventContext, event: &Event) -> Result<bool> { self.lock().event(size, context, event) }
}

#[cfg(feature="ui")] impl Widget for Player {
	#[throws] fn paint(&mut self, target: &mut Target, size: size, _: int2) {
		/*let _ : Result<()> = try {
			let path = url::Url::parse(self.metadata.get("mpris:artUrl").ok_or("Missing cover")?)?;
			let path = path.to_file_path().expect("Expecting local cover");
			let image = image_io::io::Reader::open(path)?.decode()?.into_rgb8();
			let source = image::Image::<&[image::rgb::<u8>]>::cast_slice(&image, image.dimensions().into());
			let mut target = {let size = fit(size, source.size); target.slice_mut((target.size-size)/2, size)};
			let ref map = image::sRGB_to_PQ10;
			let [num, den] = if source.size.x*target.size.y > source.size.y*target.size.x { [source.size.x, target.size.x] } else { [source.size.y, target.size.y] };
			target.set(|p| image::rgb8_to_10(map, source[p*num/den]));
		};*/
		if !self.output.playing() {
			let size = std::cmp::min(target.size.x, target.size.y).into();
			let mut target = target.slice_mut((target.size-size)/2, size);
			use image::xy;
			target.slice_mut(size*xy{x:1, y:1}/xy{x:5, y:5}, size*xy{x:1, y:3}/xy{x:5, y:5}).fill(white.into());
			target.slice_mut(size*xy{x:3, y:1}/xy{x:5, y:5}, size*xy{x:1, y:3}/xy{x:5, y:5}).fill(white.into());
		}
		if !self.metadata.is_empty() {
			let mut text = text(self.title().expect(&format!("{:?}",self.metadata)), &[]);
			let text_size = fit(size, text.size());
			text.paint_fit(target, target.size, xy{x: 0, y: (size.y as i32-text_size.y as i32)/2});
		}
	}
	#[throws] fn event(&mut self, _: size, ctx: &mut EventContext, event: &Event) -> bool {
		match event {
			Event::Key(' ') => { self.output.toggle_play_pause()?; true },
			Event::Trigger => { ctx.toplevel.set_title(self.title()?); true }
			_ => false
		}
	}
}

#[cfg(feature="zbus")] #[async_std::main]
async fn main() -> Result {
	let mut player : Arch<Player> = default();
	#[cfg(feature="zbus")] let _mpris = zbus::ConnectionBuilder::session()?.name("org.mpris.MediaPlayer2.RustMusic")?.serve_at("/org/zbus/RustMusic", Arch::clone(&player))?.internal_executor(true).build().await?;
	let ref app = App::new()?;
	thread::scope(|s| {
		use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
		let stop = AtomicBool::new(false);
		/*thread::Builder::new().spawn_scoped(s, {let player : Arch<Player> = Arch::clone(&player); move || Result::<()>::unwrap(try {
		})})?;*/
		app.run("Player", &mut player).inspect_err(|e| println!("{e:?}"))
	})
}
