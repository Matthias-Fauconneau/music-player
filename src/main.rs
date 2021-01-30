use {anyhow::{Error, Result}, fehler::throws};

#[throws] fn visit(path: &std::path::Path, f: &mut impl FnMut(&std::fs::DirEntry) -> Result<()>) {
	if path.is_dir() {
		for entry in std::fs::read_dir(path)? {
			let entry = entry?;
			let path = entry.path();
			if path.is_dir() { visit(&path, f)?; } else { f(&entry)?; }
		}
	}
}

mod audio;

#[throws] fn play(output: &mut audio::Output, entry: &std::fs::DirEntry) {
	let mut reader = claxon::FlacReader::open(entry.path())?;
	assert_eq!(reader.streaminfo().sample_rate, output.rate);
	let mut samples = reader.samples().map(|v| v.unwrap() as i16);
	while output.write(&mut samples) > 0 {}
}

#[throws] fn main() {
	let mut output = audio::output()?;
	visit(&std::env::current_dir()?, &mut move |entry| play(&mut output, entry))?
}
