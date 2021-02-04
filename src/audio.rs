use fehler::throws;

pub struct Output {
	pub device: alsa::PCM,
	pub output: alsa::direct::pcm::MmapPlayback<i16>,
}

impl Output {

#[throws(alsa::Error)] pub fn new(rate: u32) -> Self {
	let device = alsa::PCM::new("hw:1,0", alsa::Direction::Playback, false)?;
	{let hardware_parameters = alsa::pcm::HwParams::any(&device)?;
		hardware_parameters.set_rate(rate, alsa::ValueOr::Nearest)?;
		hardware_parameters.set_channels(2)?;
		hardware_parameters.set_format(alsa::pcm::Format::s16())?;
		hardware_parameters.set_access(alsa::pcm::Access::MMapInterleaved)?;
		hardware_parameters.set_buffer_size(hardware_parameters.get_buffer_size_max()?)?;
		hardware_parameters.set_periods(2, alsa::ValueOr::Greater)?;
		device.hw_params(&hardware_parameters)?;}
	{let software_parameters = device.sw_params_current()?;
	 software_parameters.set_avail_min(device.hw_params_current()?.get_period_size()?)?;
   device.sw_params(&software_parameters)?;}
	//device.prepare()?;
	let output = device.direct_mmap_playback::<i16>()?;
	Self{device, output}
}

#[throws(alsa::Error)] pub async fn write(&self, frames: &mut impl ExactSizeIterator<Item=(i16, i16)>) -> usize {
	assert!(frames.len() > 0, "{}", frames.len());
	let (buffer, _) = self.output.data_ptr();
	let buffer = unsafe{std::slice::from_raw_parts_mut(buffer.ptr as *mut [i16; 2], buffer.frames as usize)};
	assert!(buffer.len() > 0, "{}", buffer.len());
	let target = buffer.into_iter().zip(frames);
	let len = target.len();
	for (target, frame) in target { unsafe{std::ptr::write_volatile(target as *mut [i16; 2], [frame.0, frame.1])}; }
	self.output.commit(len as alsa::pcm::Frames);
	if self.device.state() == alsa::pcm::State::Prepared { self.device.start()?; }
	assert!({use alsa::pcm::State::*; matches!(self.device.state(), Running|Paused)});
	async_io::Async::new(alsa::PollDescriptors::get(&self.device)?[0].fd).unwrap().writable().await.unwrap();
	len
}

pub fn playing(self: &Output) -> bool { self.device.state() == alsa::pcm::State::Running }
pub fn toggle_play_pause(self: &Output) -> alsa::Result<()> { self.device.pause(self.playing()) }

}
