/*
 * Copyright 2026 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "livekit/audio_device_controller.h"

#include <string>
#include <utility>

namespace livekit_ffi {

AudioDeviceController::AudioDeviceController(
    webrtc::scoped_refptr<AdmProxy> adm_proxy)
    : adm_proxy_(std::move(adm_proxy)) {}

int16_t AudioDeviceController::playout_devices() const {
  return adm_proxy_->PlayoutDevices();
}

int16_t AudioDeviceController::recording_devices() const {
  return adm_proxy_->RecordingDevices();
}

rust::String AudioDeviceController::playout_device_name(uint16_t index) const {
  char name[webrtc::kAdmMaxDeviceNameSize] = {0};
  char guid[webrtc::kAdmMaxGuidSize] = {0};
  adm_proxy_->PlayoutDeviceName(index, name, guid);
  return rust::String(name);
}

rust::String AudioDeviceController::recording_device_name(uint16_t index) const {
  char name[webrtc::kAdmMaxDeviceNameSize] = {0};
  char guid[webrtc::kAdmMaxGuidSize] = {0};
  adm_proxy_->RecordingDeviceName(index, name, guid);
  return rust::String(name);
}

rust::String AudioDeviceController::playout_device_guid(uint16_t index) const {
  char name[webrtc::kAdmMaxDeviceNameSize] = {0};
  char guid[webrtc::kAdmMaxGuidSize] = {0};
  adm_proxy_->PlayoutDeviceName(index, name, guid);
  return rust::String(guid);
}

rust::String AudioDeviceController::recording_device_guid(uint16_t index) const {
  char name[webrtc::kAdmMaxDeviceNameSize] = {0};
  char guid[webrtc::kAdmMaxGuidSize] = {0};
  adm_proxy_->RecordingDeviceName(index, name, guid);
  return rust::String(guid);
}

bool AudioDeviceController::set_playout_device(uint16_t index) const {
  return adm_proxy_->SetPlayoutDevice(index) == 0;
}

bool AudioDeviceController::set_recording_device(uint16_t index) const {
  return adm_proxy_->SetRecordingDevice(index) == 0;
}

bool AudioDeviceController::set_playout_device_by_guid(rust::String guid) const {
  int16_t count = adm_proxy_->PlayoutDevices();

  // Try to find a device matching the GUID
  for (int16_t i = 0; i < count; i++) {
    char name[webrtc::kAdmMaxDeviceNameSize] = {0};
    char device_guid[webrtc::kAdmMaxGuidSize] = {0};
    if (adm_proxy_->PlayoutDeviceName(i, name, device_guid) == 0) {
      if (std::string(guid.c_str()) == std::string(device_guid)) {
        return adm_proxy_->SetPlayoutDevice(i) == 0;
      }
    }
  }

  // No match found - fall back to default device (index 0).
  // This handles mobile platforms (iOS/Android) where:
  // - GUIDs may be empty or not meaningful
  // - Device selection is a no-op (system handles routing)
  if (count > 0) {
    return adm_proxy_->SetPlayoutDevice(0) == 0;
  }
  return false;
}

bool AudioDeviceController::set_recording_device_by_guid(rust::String guid) const {
  int16_t count = adm_proxy_->RecordingDevices();

  // Try to find a device matching the GUID
  for (int16_t i = 0; i < count; i++) {
    char name[webrtc::kAdmMaxDeviceNameSize] = {0};
    char device_guid[webrtc::kAdmMaxGuidSize] = {0};
    if (adm_proxy_->RecordingDeviceName(i, name, device_guid) == 0) {
      if (std::string(guid.c_str()) == std::string(device_guid)) {
        return adm_proxy_->SetRecordingDevice(i) == 0;
      }
    }
  }

  // No match found - fall back to default device (index 0).
  // This handles mobile platforms (iOS/Android) where:
  // - GUIDs may be empty or not meaningful
  // - Device selection is a no-op (system handles routing)
  if (count > 0) {
    return adm_proxy_->SetRecordingDevice(0) == 0;
  }
  return false;
}

bool AudioDeviceController::stop_recording() const {
  return adm_proxy_->StopRecording() == 0;
}

bool AudioDeviceController::init_recording() const {
  return adm_proxy_->InitRecording() == 0;
}

bool AudioDeviceController::start_recording() const {
  return adm_proxy_->StartRecording() == 0;
}

bool AudioDeviceController::recording_is_initialized() const {
  return adm_proxy_->RecordingIsInitialized();
}

bool AudioDeviceController::stop_playout() const {
  return adm_proxy_->StopPlayout() == 0;
}

bool AudioDeviceController::init_playout() const {
  return adm_proxy_->InitPlayout() == 0;
}

bool AudioDeviceController::start_playout() const {
  return adm_proxy_->StartPlayout() == 0;
}

bool AudioDeviceController::playout_is_initialized() const {
  return adm_proxy_->PlayoutIsInitialized();
}

bool AudioDeviceController::builtin_aec_is_available() const {
  return adm_proxy_->BuiltInAECIsAvailable();
}

bool AudioDeviceController::builtin_agc_is_available() const {
  return adm_proxy_->BuiltInAGCIsAvailable();
}

bool AudioDeviceController::builtin_ns_is_available() const {
  return adm_proxy_->BuiltInNSIsAvailable();
}

bool AudioDeviceController::enable_builtin_aec(bool enable) const {
  return adm_proxy_->EnableBuiltInAEC(enable) == 0;
}

bool AudioDeviceController::enable_builtin_agc(bool enable) const {
  return adm_proxy_->EnableBuiltInAGC(enable) == 0;
}

bool AudioDeviceController::enable_builtin_ns(bool enable) const {
  return adm_proxy_->EnableBuiltInNS(enable) == 0;
}

void AudioDeviceController::set_adm_recording_enabled(bool enabled) const {
  adm_proxy_->set_recording_enabled(enabled);
}

bool AudioDeviceController::adm_recording_enabled() const {
  return adm_proxy_->recording_enabled();
}

void AudioDeviceController::set_adm_playout_enabled(bool enabled) const {
  adm_proxy_->set_playout_enabled(enabled);
}

bool AudioDeviceController::adm_playout_enabled() const {
  return adm_proxy_->playout_enabled();
}

bool AudioDeviceController::acquire_platform_adm() const {
  return adm_proxy_->AcquirePlatformAdm();
}

void AudioDeviceController::release_platform_adm() const {
  adm_proxy_->ReleasePlatformAdm();
}

int AudioDeviceController::platform_adm_ref_count() const {
  return adm_proxy_->platform_adm_ref_count();
}

bool AudioDeviceController::is_platform_adm_active() const {
  return adm_proxy_->is_platform_adm_active();
}

}  // namespace livekit_ffi
