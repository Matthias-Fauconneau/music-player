use {fehler::throws, zbus::Error, zbus_macros::dbus_interface, std::default::default};
struct Player<'t>(&'t super::Player);
unsafe fn extend_lifetime<'t>(r: Player<'t>) -> Player<'static> { std::mem::transmute::<Player<'t>, Player<'static>>(r) }
pub struct Drop<'t>(&'t std::cell::RefCell<zbus::ObjectServer>);
#[throws] pub fn at<'t>(object_server: &'t std::cell::RefCell<zbus::ObjectServer>, player: &'t super::Player) -> Drop<'t> {
	object_server.borrow_mut().at("/org/mpris/MediaPlayer2", unsafe{extend_lifetime(Player(player))})?;
	Drop(object_server)
}
impl std::ops::Drop for Drop<'_> { fn drop(&mut self) { dbg!(); self.0.borrow_mut().remove::<Player<'static>, _>("/org/mpris/MediaPlayer2").unwrap(); } }
#[dbus_interface(name= "org.mpris.MediaPlayer2.Player")]
impl Player<'static> {
    fn next(&self)  {}
    fn open_uri(&self, _uri: &str) {}
    fn pause(&self) {}
    fn play(&self) {}
    fn play_pause(&self) {}
    fn previous(&self) {}
    fn seek(&self, _offset: i64) {}
    //fn set_position(&self, _track_id: &zvariant::ObjectPath, _position: i64) {}
    fn stop(&self) {}
    #[dbus_interface(signal)] fn seeked(&self, _position: i64) -> zbus::Result<()>;
    #[dbus_interface(property)] fn can_control(&self) -> bool { false }
    #[dbus_interface(property)] fn can_go_next(&self) -> bool { false }
    #[dbus_interface(property)] fn can_go_previous(&self) -> bool { false }
    #[dbus_interface(property)] fn can_pause(&self) -> bool { false }
    #[dbus_interface(property)] fn can_play(&self) -> bool { false }
    #[dbus_interface(property)] fn can_seek(&self) -> bool { false }
    #[dbus_interface(property)] fn loop_status(&self) -> String { default() }
    #[dbus_interface(property)] fn set_loop_status(&self, _value: &str) {}
    #[dbus_interface(property)] fn maximum_rate(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn metadata(&self) -> std::collections::HashMap<String, zvariant::Value> { dbg!(); default() }
    #[dbus_interface(property)] fn minimum_rate(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn playback_status(&self) -> String { dbg!(); default() }
    #[dbus_interface(property)] fn position(&self) -> f64 { 0. }
    #[dbus_interface(property)] fn rate(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn set_rate(&self, _: f64) {}
    #[dbus_interface(property)] fn shuffle(&self) -> bool { false }
    #[dbus_interface(property)] fn set_shuffle(&self, _: bool) {}
    #[dbus_interface(property)] fn volume(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn set_volume(&self, _: f64) {}
}
