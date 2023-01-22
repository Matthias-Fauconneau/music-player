use zbus::{dbus_interface, zvariant::{ObjectPath,Value}}; //Result
#[dbus_interface(name="org.mpris.MediaPlayer2.Player")]
impl super::Arch<super::Player> {
    fn next(&self)  {}
    fn open_uri(&self, _uri: &str) {}
    fn pause(&self) { self.lock().audio.device.pause(true).unwrap(); }
    fn play(&self) { self.lock().audio.device.pause(false).unwrap(); }
    fn play_pause(&self) { self.lock().audio.toggle_play_pause().unwrap(); }
    fn previous(&self) {}
    fn seek(&self, _offset: i64) {}
    fn set_position(&self, _track_id: ObjectPath, _position: i64) {}
    fn stop(&self) {}
    //#[dbus_interface(signal)] async fn seeked(&self, _position: i64) -> Result<()>;
    #[dbus_interface(property)] fn can_control(&self) -> bool { true }
    #[dbus_interface(property)] fn can_go_next(&self) -> bool { false }
    #[dbus_interface(property)] fn can_go_previous(&self) -> bool { false }
    #[dbus_interface(property)] fn can_pause(&self) -> bool { self.lock().audio.playing() }
    #[dbus_interface(property)] fn can_play(&self) -> bool { true }
    #[dbus_interface(property)] fn can_seek(&self) -> bool { false }
    #[dbus_interface(property)] fn loop_status(&self) -> String { "None".into() }
    #[dbus_interface(property)] fn set_loop_status(&self, _value: &str) {}
    #[dbus_interface(property)] fn maximum_rate(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn metadata(&self) -> std::collections::HashMap<String, Value> {
        self.lock().metadata.iter().map(|(k ,v)| (k.clone(), v.to_owned().into())).collect()
    }
    #[dbus_interface(property)] fn minimum_rate(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn playback_status(&self) -> String { if self.lock().audio.playing() {"Playing"} else {"Paused"}.into()  }
    #[dbus_interface(property)] fn position(&self) -> f64 { 0. }
    #[dbus_interface(property)] fn rate(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn set_rate(&self, _: f64) {}
    #[dbus_interface(property)] fn shuffle(&self) -> bool { false }
    #[dbus_interface(property)] fn set_shuffle(&self, _: bool) {}
    #[dbus_interface(property)] fn volume(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn set_volume(&self, _: f64) {}
}
