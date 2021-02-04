use {fehler::throws, zbus::{Error, dbus_interface, export::*}};
use super::Player as Object;
unsafe fn extend_lifetime<'t>(r: Object<'t>) -> Object<'static> { std::mem::transmute::<Object<'t>, Object<'static>>(r) }
pub struct Drop<'t>(&'t std::cell::RefCell<zbus::ObjectServer>);
#[throws] pub fn at<'t>(object_server: &'t std::cell::RefCell<zbus::ObjectServer>, object: Object<'t>) -> Drop<'t> {
    object_server.borrow_mut().at(Object::PATH, unsafe{extend_lifetime(object)})?;
    Drop(object_server)
}
impl std::ops::Drop for Drop<'_> { fn drop(&mut self) { self.0.borrow_mut().remove::<Object<'static>, _>(Object::PATH).unwrap(); } }
#[dbus_interface(name= "org.mpris.MediaPlayer2.Player")]
impl Object<'static> {
    fn next(&self)  {}
    fn open_uri(&self, _uri: &str) {}
    fn pause(&self) { self.audio.borrow().device.pause(true).unwrap(); }
    fn play(&self) { self.audio.borrow().device.pause(false).unwrap(); }
    fn play_pause(&self) { self.audio.borrow().toggle_play_pause().unwrap(); }
    fn previous(&self) {}
    fn seek(&self, _offset: i64) {}
    fn set_position(&self, _track_id: zvariant::ObjectPath, _position: i64) {}
    fn stop(&self) {}
    #[dbus_interface(signal)] fn seeked(&self, _position: i64) -> zbus::Result<()>;
    #[dbus_interface(property)] fn can_control(&self) -> bool { true }
    #[dbus_interface(property)] fn can_go_next(&self) -> bool { false }
    #[dbus_interface(property)] fn can_go_previous(&self) -> bool { false }
    #[dbus_interface(property)] fn can_pause(&self) -> bool { self.audio.borrow().playing() }
    #[dbus_interface(property)] fn can_play(&self) -> bool { true }
    #[dbus_interface(property)] fn can_seek(&self) -> bool { false }
    #[dbus_interface(property)] fn loop_status(&self) -> String { "None".into() }
    #[dbus_interface(property)] fn set_loop_status(&self, _value: &str) {}
    #[dbus_interface(property)] fn maximum_rate(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn metadata(&self) -> std::collections::HashMap<String, zvariant::Value> { self.metadata.iter().map(|(k ,v)| (k.clone(), v.to_owned().into())).collect() }
    #[dbus_interface(property)] fn minimum_rate(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn playback_status(&self) -> String { if self.audio.borrow().playing() {"Playing"} else {"Paused"}.into()  }
    #[dbus_interface(property)] fn position(&self) -> f64 { 0. }
    #[dbus_interface(property)] fn rate(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn set_rate(&self, _: f64) {}
    #[dbus_interface(property)] fn shuffle(&self) -> bool { false }
    #[dbus_interface(property)] fn set_shuffle(&self, _: bool) {}
    #[dbus_interface(property)] fn volume(&self) -> f64 { 1. }
    #[dbus_interface(property)] fn set_volume(&self, _: f64) {}
}
