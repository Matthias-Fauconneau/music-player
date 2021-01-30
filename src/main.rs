#[fehler::throws(anyhow::Error)] fn visit(path: &std::path::Path, f: &impl Fn(&std::fs::DirEntry) -> anyhow::Result<()>) {
	if path.is_dir() {
		for entry in std::fs::read_dir(path)? {
			let entry = entry?;
			let path = entry.path();
			if path.is_dir() { visit(&path, f)?; } else { f(&entry)?; }
		}
	}
}

#[fehler::throws(anyhow::Error)] fn main() {
	let (_, output) = rodio::OutputStream::try_default()?;
	#[fehler::throws(anyhow::Error)] fn play(output: &rodio::OutputStreamHandle, entry: &std::fs::DirEntry) {
		use anyhow::Context;
		let sink = output.play_once(std::io::BufReader::new(std::fs::File::open(entry.path())?)).context(entry.path().to_str().unwrap().to_owned())?;
    sink.sleep_until_end();
	}
	visit(&std::env::current_dir()?, &|entry| play(&output, entry))?
}
