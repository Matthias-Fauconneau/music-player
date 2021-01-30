use fehler::throws;

#[derive(derive_more::Deref, derive_more::DerefMut)] pub struct Output {
	pub rate: u32,
	#[deref]#[deref_mut] output: alsa::direct::pcm::MmapPlayback<i16>,
}

#[throws(anyhow::Error)] pub fn output() -> Output {
	let device = alsa::PCM::new("hw:1", alsa::Direction::Playback, false)?;
	{let hardware_parameters = alsa::pcm::HwParams::any(&device)?;
		hardware_parameters.set_channels(2)?;
		//hardware_parameters.set_rate(rate, alsa::ValueOr::Greater)?;
		hardware_parameters.set_format(alsa::pcm::Format::s16())?;
		hardware_parameters.set_access(alsa::pcm::Access::MMapInterleaved)?;
		//hardware_parameters.set_buffer_size(2K)?;
		//hardware_parameters.set_period_size(/2, alsa::ValueOr::Nearest)?;
		device.hw_params(&hardware_parameters)?;}
	let hardware_parameters = device.hw_params_current()?;
	{let software_parameters = device.sw_params_current()?;
		software_parameters.set_avail_min(hardware_parameters.get_period_size()?)?;
		device.sw_params(&software_parameters)?;}
	let mut output = device.direct_mmap_playback::<i16>()?;
	device.prepare()?;
	//device.avail() > 0
	output.write(&mut std::iter::repeat(0));
	//assert(device.status().state() == alsa::pcm::State::Prepared); //Running
	device.start()?;
	Output{rate: hardware_parameters.get_rate()?, output}
}
