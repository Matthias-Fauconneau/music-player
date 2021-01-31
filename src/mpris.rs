pub mod media_player2;
use {fehler::throws, dbus::MethodErr as Error, std::default::default};
use super::*;
impl media_player2::OrgMprisMediaPlayer2Player for std::sync::Weak<Player> {
	#[throws] fn next(&mut self) { dbg!() }
	#[throws] fn previous(&mut self) {}
	#[throws] fn pause(&mut self) {}
	#[throws] fn play_pause(&mut self) { self.upgrade().map(|p| super::toggle_play_pause(&p.device.lock().unwrap()).unwrap()).unwrap() }
	#[throws] fn stop(&mut self) {}
	#[throws] fn play(&mut self) {}
	#[throws] fn seek(&mut self, _offset: i64) {}
	#[throws] fn set_position(&mut self, _track_id: dbus::Path<'static>, _position: i64) {}
	#[throws] fn open_uri(&mut self, _uri: String) {}
	#[throws] fn playback_status(&self) -> String { if let Some(true) = self.upgrade().map(|p| playing(&p.device.lock().unwrap())) { "Playing" } else { "Paused" }.into() }
	#[throws] fn loop_status(&self) -> String { default() }
	#[throws] fn set_loop_status(&self, _value: String) {}
	#[throws] fn rate(&self) -> f64 { default() }
	#[throws] fn set_rate(&self, _value: f64) {}
	#[throws] fn shuffle(&self) -> bool { default() }
	#[throws] fn set_shuffle(&self, _value: bool) {}
	#[throws] fn metadata(&self) -> dbus::arg::PropMap {
		self.upgrade().map(|p| p.metadata.lock().unwrap().iter().map(
			|(k ,v)| (k.clone(), dbus::arg::Variant(Box::new(v.clone()) as Box<dyn dbus::arg::RefArg>))
		).collect()).unwrap()
	}
	#[throws] fn volume(&self) -> f64 { default() }
	#[throws] fn set_volume(&self, _value: f64) {}
	#[throws] fn position(&self) -> i64 { default() }
	#[throws] fn minimum_rate(&self) -> f64 { default() }
	#[throws] fn maximum_rate(&self) -> f64 { default() }
	#[throws] fn can_go_next(&self) -> bool { default() }
	#[throws] fn can_go_previous(&self) -> bool { default() }
	#[throws] fn can_play(&self) -> bool { default() }
	#[throws] fn can_pause(&self) -> bool { default() }
	#[throws] fn can_seek(&self) -> bool { default() }
	#[throws] fn can_control(&self) -> bool { default() }
}
