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

#pragma once

#include <memory>

#include "api/scoped_refptr.h"
#include "livekit/adm_proxy.h"
#include "rust/cxx.h"

namespace livekit_ffi {

class AudioDeviceController {
 public:
  explicit AudioDeviceController(webrtc::scoped_refptr<AdmProxy> adm_proxy);

  // Device enumeration
  int16_t playout_devices() const;
  int16_t recording_devices() const;
  rust::String playout_device_name(uint16_t index) const;
  rust::String recording_device_name(uint16_t index) const;
  rust::String playout_device_guid(uint16_t index) const;
  rust::String recording_device_guid(uint16_t index) const;

  // Device selection
  bool set_playout_device(uint16_t index) const;
  bool set_recording_device(uint16_t index) const;
  bool set_playout_device_by_guid(rust::String guid) const;
  bool set_recording_device_by_guid(rust::String guid) const;

  // Recording control
  bool stop_recording() const;
  bool init_recording() const;
  bool start_recording() const;
  bool recording_is_initialized() const;

  // Playout control
  bool stop_playout() const;
  bool init_playout() const;
  bool start_playout() const;
  bool playout_is_initialized() const;

  // Built-in audio processing
  bool builtin_aec_is_available() const;
  bool builtin_agc_is_available() const;
  bool builtin_ns_is_available() const;
  bool enable_builtin_aec(bool enable) const;
  bool enable_builtin_agc(bool enable) const;
  bool enable_builtin_ns(bool enable) const;

  // ADM recording control
  void set_adm_recording_enabled(bool enabled) const;
  bool adm_recording_enabled() const;

  // ADM playout control
  void set_adm_playout_enabled(bool enabled) const;
  bool adm_playout_enabled() const;

  // Platform ADM lifecycle management
  bool acquire_platform_adm() const;
  void release_platform_adm() const;
  int platform_adm_ref_count() const;
  bool is_platform_adm_active() const;

 private:
  webrtc::scoped_refptr<AdmProxy> adm_proxy_;
};

}  // namespace livekit_ffi
