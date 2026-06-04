// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pub use cxx::SharedPtr;

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/audio_device_controller.h");
        include!("livekit/peer_connection_factory.h");

        type AudioDeviceController;
        type PeerConnectionFactory = crate::peer_connection_factory::ffi::PeerConnectionFactory;

        fn audio_device(self: &PeerConnectionFactory) -> SharedPtr<AudioDeviceController>;

        fn playout_devices(self: &AudioDeviceController) -> i16;
        fn recording_devices(self: &AudioDeviceController) -> i16;
        fn playout_device_name(self: &AudioDeviceController, index: u16) -> String;
        fn recording_device_name(self: &AudioDeviceController, index: u16) -> String;
        fn playout_device_guid(self: &AudioDeviceController, index: u16) -> String;
        fn recording_device_guid(self: &AudioDeviceController, index: u16) -> String;

        fn set_playout_device(self: &AudioDeviceController, index: u16) -> bool;
        fn set_recording_device(self: &AudioDeviceController, index: u16) -> bool;
        fn set_playout_device_by_guid(self: &AudioDeviceController, guid: String) -> bool;
        fn set_recording_device_by_guid(self: &AudioDeviceController, guid: String) -> bool;

        fn stop_recording(self: &AudioDeviceController) -> bool;
        fn init_recording(self: &AudioDeviceController) -> bool;
        fn start_recording(self: &AudioDeviceController) -> bool;
        fn recording_is_initialized(self: &AudioDeviceController) -> bool;

        fn stop_playout(self: &AudioDeviceController) -> bool;
        fn init_playout(self: &AudioDeviceController) -> bool;
        fn start_playout(self: &AudioDeviceController) -> bool;
        fn playout_is_initialized(self: &AudioDeviceController) -> bool;

        fn builtin_aec_is_available(self: &AudioDeviceController) -> bool;
        fn builtin_agc_is_available(self: &AudioDeviceController) -> bool;
        fn builtin_ns_is_available(self: &AudioDeviceController) -> bool;
        fn enable_builtin_aec(self: &AudioDeviceController, enable: bool) -> bool;
        fn enable_builtin_agc(self: &AudioDeviceController, enable: bool) -> bool;
        fn enable_builtin_ns(self: &AudioDeviceController, enable: bool) -> bool;

        fn set_adm_recording_enabled(self: &AudioDeviceController, enabled: bool);
        fn adm_recording_enabled(self: &AudioDeviceController) -> bool;

        fn set_adm_playout_enabled(self: &AudioDeviceController, enabled: bool);
        fn adm_playout_enabled(self: &AudioDeviceController) -> bool;

        fn acquire_platform_adm(self: &AudioDeviceController) -> bool;
        fn release_platform_adm(self: &AudioDeviceController);
        fn platform_adm_ref_count(self: &AudioDeviceController) -> i32;
        fn is_platform_adm_active(self: &AudioDeviceController) -> bool;
    }
}

impl_thread_safety!(ffi::AudioDeviceController, Send + Sync);

#[cfg(test)]
mod tests {
    use crate::peer_connection_factory::ffi::create_peer_connection_factory;
    use std::sync::Mutex;

    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_audio_device_controller_bridge() {
        let _guard = TEST_MUTEX.lock().expect("test mutex poisoned");
        let factory = create_peer_connection_factory();
        let audio = factory.audio_device();

        let recording_count = audio.recording_devices();
        let playout_count = audio.playout_devices();
        assert!(recording_count >= 0);
        assert!(playout_count >= 0);

        if recording_count > 0 {
            let _ = audio.recording_device_name(0);
            let guid = audio.recording_device_guid(0);
            let _ = audio.set_recording_device_by_guid(guid);
        }

        if playout_count > 0 {
            let _ = audio.playout_device_name(0);
            let guid = audio.playout_device_guid(0);
            let _ = audio.set_playout_device_by_guid(guid);
        }

        let initial_recording = audio.adm_recording_enabled();
        audio.set_adm_recording_enabled(!initial_recording);
        assert_eq!(audio.adm_recording_enabled(), !initial_recording);
        audio.set_adm_recording_enabled(initial_recording);
        assert_eq!(audio.adm_recording_enabled(), initial_recording);

        let initial_playout = audio.adm_playout_enabled();
        audio.set_adm_playout_enabled(!initial_playout);
        assert_eq!(audio.adm_playout_enabled(), !initial_playout);
        audio.set_adm_playout_enabled(initial_playout);
        assert_eq!(audio.adm_playout_enabled(), initial_playout);

        assert!(audio.platform_adm_ref_count() >= 0);
        let _ = audio.is_platform_adm_active();
    }
}
