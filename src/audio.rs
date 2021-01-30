use fehler::throws;

pub struct Output {
	pub device: alsa::PCM,
	output: alsa::direct::pcm::MmapPlayback<i16>,
}

impl Output {
#[throws(alsa::Error)] pub fn new() -> Self {
	let device = alsa::PCM::new("hw:1,0", alsa::Direction::Playback, false)?;
	{let hardware_parameters = alsa::pcm::HwParams::any(&device)?;
		hardware_parameters.set_rate(44100, alsa::ValueOr::Nearest)?;
		hardware_parameters.set_channels(2)?;
		hardware_parameters.set_format(alsa::pcm::Format::s16())?;
		hardware_parameters.set_access(alsa::pcm::Access::MMapInterleaved)?;
		hardware_parameters.set_buffer_size(hardware_parameters.get_buffer_size_max()?)?;
		hardware_parameters.set_periods(2, alsa::ValueOr::Greater)?;
		device.hw_params(&hardware_parameters)?;}
	{let hardware_parameters = device.hw_params_current()?;
	{let software_parameters = device.sw_params_current()?;
		software_parameters.set_avail_min(hardware_parameters.get_period_size()?)?;
		device.sw_params(&software_parameters)?;}}
	device.prepare()?;
	let output = device.direct_mmap_playback::<i16>()?;
	Self{device, output}
}

#[throws(alsa::Error)] pub fn write(&mut self, frames: &mut impl ExactSizeIterator<Item=(i16, i16)>) -> usize {
	assert!(frames.len() > 0, "{}", frames.len());
	let (buffer, _) = self.output.data_ptr();
	let buffer = unsafe{std::slice::from_raw_parts_mut(buffer.ptr as *mut [i16; 2], buffer.frames as usize)};
	assert!(buffer.len() > 0, "{}", buffer.len());
	let target = buffer.into_iter().zip(frames);
	let len = target.len();
	for (target, frame) in target { unsafe{std::ptr::write_volatile(target as *mut [i16; 2], [frame.0, frame.1])}; }
	self.output.commit(len as alsa::pcm::Frames);
	if self.output.status().state() == alsa::pcm::State::Prepared { self.device.start()?; }
	assert!({use alsa::pcm::State::*; matches!(self.output.status().state(), Running|Paused)});
	len
}

}
