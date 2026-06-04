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

#include "livekit/adm_proxy.h"

#include "api/audio/audio_device.h"
#include "api/audio/create_audio_device_module.h"
#include "api/make_ref_counted.h"
#include "rtc_base/logging.h"
#include "rtc_base/thread.h"

#if defined(__ANDROID__)
#include <jni.h>
#include "sdk/android/native_api/audio_device_module/audio_device_android.h"
#include "sdk/android/native_api/base/init.h"
#endif

namespace livekit_ffi {

AdmProxy::AdmProxy(const webrtc::Environment& env, webrtc::Thread* worker_thread)
    : env_(env),
      worker_thread_(worker_thread) {
  // Create the synthetic ADM for synthetic mode. SyntheticAudioDevice pumps
  // the WebRTC audio pipeline without platform audio, allowing FFI callbacks
  // to receive decoded remote audio.
  synthetic_adm_ = webrtc::make_ref_counted<SyntheticAudioDevice>(env_);
  if (synthetic_adm_->Init() != 0) {
    RTC_LOG(LS_ERROR) << "AdmProxy: Failed to initialize synthetic ADM";
  }

  // Create the Platform ADM for real audio I/O.
  // This is created immediately (not lazily) for iOS compatibility.
  // iOS audio session requires early setup to avoid KVO race conditions.
  // On Android, we defer Platform ADM creation to AcquirePlatformAdm().
  // This is because:
  // 1. CreateAudioDeviceModule requires JNI to be fully initialized
  // 2. The JNI initialization (via JNI_OnLoad or manual init) may not have
  //    completed by the time the AdmProxy constructor runs
  // 3. Deferring creation ensures JNI is ready when we actually need the ADM
#if defined(__ANDROID__)
  // platform_adm_ stays nullptr, will be created in EnsurePlatformAdmCreated()
#else
  platform_adm_ = webrtc::CreateAudioDeviceModule(
      env_, webrtc::AudioDeviceModule::kPlatformDefaultAudio);

  if (!platform_adm_) {
    RTC_LOG(LS_ERROR) << "AdmProxy: CreateAudioDeviceModule returned nullptr";
  } else {
    int32_t init_result = platform_adm_->Init();
    if (init_result != 0) {
      RTC_LOG(LS_ERROR) << "AdmProxy: Platform ADM Init() failed with error=" << init_result;
      platform_adm_ = nullptr;
    }
  }
#endif
}

AdmProxy::~AdmProxy() {
  RTC_LOG(LS_VERBOSE) << "AdmProxy::~AdmProxy()";

  if (synthetic_adm_) {
    synthetic_adm_->Terminate();
    synthetic_adm_ = nullptr;
  }

  if (platform_adm_) {
    platform_adm_->Terminate();
    platform_adm_ = nullptr;
  }
}

// =============================================================================
// Helper Methods
// =============================================================================

bool AdmProxy::is_platform_playout_active() const {
  // Platform playout is active when: ref_count > 0 AND playout explicitly enabled.
  // Otherwise, synthetic mode handles playout via the internal pumping task.
  return platform_adm_ && platform_adm_ref_count_ > 0 && playout_enabled_;
}

webrtc::AudioDeviceModule* AdmProxy::recording_adm() const {
  // Recording only available through platform ADM when enabled.
  // Synthetic mode doesn't support recording (no microphone).
  if (platform_adm_ && platform_adm_ref_count_ > 0 && recording_enabled_) {
    return platform_adm_.get();
  }
  return nullptr;
}

// =============================================================================
// Platform ADM Lifecycle Management
// =============================================================================

#if defined(__ANDROID__)
// Lazily creates the Platform ADM on Android. Must be called with mutex held.
// Returns true if ADM is available (either already existed or successfully created).
bool AdmProxy::EnsurePlatformAdmCreated() {
  if (platform_adm_) {
    return true;  // Already created
  }

  // Use CreateAndroidAudioDeviceModule which properly uses GetAppContext()
  // to get the application context set via ContextUtils.initialize().
  platform_adm_ = webrtc::CreateAndroidAudioDeviceModule(
      env_, webrtc::AudioDeviceModule::kPlatformDefaultAudio);

  if (!platform_adm_) {
    RTC_LOG(LS_ERROR) << "AdmProxy: CreateAndroidAudioDeviceModule returned nullptr. "
                      << "Ensure ContextUtils.initialize() was called.";
    return false;
  }

  int32_t init_result = platform_adm_->Init();
  if (init_result != 0) {
    RTC_LOG(LS_ERROR) << "AdmProxy: Platform ADM Init() failed with error=" << init_result;
    platform_adm_ = nullptr;
    return false;
  }

  return true;
}
#endif

bool AdmProxy::AcquirePlatformAdm() {
  webrtc::MutexLock lock(&mutex_);

#if defined(__ANDROID__)
  // On Android, lazily create the Platform ADM on first acquire.
  // This ensures JNI is fully initialized before we try to create the ADM.
  if (!EnsurePlatformAdmCreated()) {
    RTC_LOG(LS_ERROR) << "AdmProxy::AcquirePlatformAdm() - Failed to create Platform ADM";
    return false;
  }
#else
  if (!platform_adm_) {
    RTC_LOG(LS_ERROR) << "AdmProxy::AcquirePlatformAdm() - Platform ADM not available";
    return false;
  }
#endif

  int old_ref_count = platform_adm_ref_count_;
  platform_adm_ref_count_++;

  // If this is the first acquisition and playout/recording is enabled,
  // we may need to switch from synthetic mode to platform ADM
  if (old_ref_count == 0) {
    SwitchPlayoutModeIfNeeded();
    SwitchRecordingAdmIfNeeded();
  }

  return true;
}

void AdmProxy::ReleasePlatformAdm() {
  webrtc::MutexLock lock(&mutex_);

  if (platform_adm_ref_count_ <= 0) {
    RTC_LOG(LS_WARNING) << "AdmProxy::ReleasePlatformAdm() called with ref_count="
                        << platform_adm_ref_count_;
    return;
  }

  platform_adm_ref_count_--;

  // If ref_count reaches 0, switch back from platform ADM to synthetic mode
  // Note: We do NOT terminate the Platform ADM - it stays alive until destructor.
  // This avoids iOS KVO race conditions from re-creating the ADM.
  if (platform_adm_ref_count_ == 0) {
    SwitchPlayoutModeIfNeeded();
    SwitchRecordingAdmIfNeeded();
  }
}

int AdmProxy::platform_adm_ref_count() const {
  webrtc::MutexLock lock(&mutex_);
  return platform_adm_ref_count_;
}

bool AdmProxy::is_platform_adm_active() const {
  webrtc::MutexLock lock(&mutex_);
  // Platform ADM is considered active when there are users and playout/recording is enabled
  return platform_adm_ != nullptr && platform_adm_ref_count_ > 0;
}

// =============================================================================
// Recording/Playout Control
// =============================================================================

void AdmProxy::set_recording_enabled(bool enabled) {
  webrtc::MutexLock lock(&mutex_);
  if (recording_enabled_ == enabled) {
    return;
  }
  recording_enabled_ = enabled;
  SwitchRecordingAdmIfNeeded();
}

bool AdmProxy::recording_enabled() const {
  webrtc::MutexLock lock(&mutex_);
  return recording_enabled_;
}

void AdmProxy::set_playout_enabled(bool enabled) {
  webrtc::MutexLock lock(&mutex_);
  if (playout_enabled_ == enabled) {
    return;
  }
  playout_enabled_ = enabled;
  SwitchPlayoutModeIfNeeded();
}

bool AdmProxy::playout_enabled() const {
  webrtc::MutexLock lock(&mutex_);
  return playout_enabled_;
}

// =============================================================================
// Mode Switching Helpers (called with mutex held)
// =============================================================================

void AdmProxy::SwitchPlayoutModeIfNeeded() {
  if (!playing_) return;

  bool use_platform = is_platform_playout_active();

  if (use_platform) {
    // Switch to platform mode - stop synthetic, start platform ADM
    if (synthetic_adm_) {
      synthetic_adm_->StopPlayout();
    }
    if (platform_adm_) {
      platform_adm_->InitPlayout();
      platform_adm_->StartPlayout();
    }
  } else {
    // Switch to synthetic mode - stop platform ADM, start synthetic ADM
    if (platform_adm_) {
      platform_adm_->StopPlayout();
    }
    if (synthetic_adm_) {
      synthetic_adm_->StartPlayout();
    }
  }
}

void AdmProxy::SwitchRecordingAdmIfNeeded() {
  if (!recording_) return;

  // Stop platform ADM recording (only one that supports recording)
  if (platform_adm_) platform_adm_->StopRecording();

  // Start if new ADM supports recording
  auto* adm = recording_adm();
  if (adm) {
    adm->InitRecording();
    adm->StartRecording();
  } else {
    recording_ = false;
  }
}

// =============================================================================
// AudioDeviceModule Interface Implementation
// =============================================================================

int32_t AdmProxy::ActiveAudioLayer(AudioLayer* audioLayer) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->ActiveAudioLayer(audioLayer);
  }
  *audioLayer = AudioLayer::kDummyAudio;
  return 0;
}

int32_t AdmProxy::RegisterAudioCallback(webrtc::AudioTransport* transport) {
  webrtc::MutexLock lock(&mutex_);
  audio_transport_ = transport;

  // Register with both ADMs so they're ready when we switch modes
  if (synthetic_adm_) {
    synthetic_adm_->RegisterAudioCallback(transport);
  }
  if (platform_adm_) {
    platform_adm_->RegisterAudioCallback(transport);
  }
  return 0;
}

int32_t AdmProxy::Init() {
  // Init is a no-op - Platform ADM is created lazily via AcquirePlatformAdm()
  return 0;
}

int32_t AdmProxy::Terminate() {
  webrtc::MutexLock lock(&mutex_);

  int32_t result = 0;
  if (synthetic_adm_) {
    result = synthetic_adm_->Terminate();
  }
  if (platform_adm_) {
    int32_t platform_result = platform_adm_->Terminate();
    if (result == 0) result = platform_result;
  }
  return result;
}

bool AdmProxy::Initialized() const {
  webrtc::MutexLock lock(&mutex_);
  // We're initialized if at least one ADM is initialized
  bool synthetic_init = synthetic_adm_ && synthetic_adm_->Initialized();
  bool platform_init = platform_adm_ && platform_adm_->Initialized();
  return synthetic_init || platform_init;
}

int16_t AdmProxy::PlayoutDevices() {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->PlayoutDevices();
  }
  // In synthetic mode, return 0 devices (no platform audio)
  return 0;
}

int16_t AdmProxy::RecordingDevices() {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->RecordingDevices();
  }
  // In synthetic mode, return 0 devices (no platform audio)
  return 0;
}

int32_t AdmProxy::PlayoutDeviceName(uint16_t index,
                                    char name[webrtc::kAdmMaxDeviceNameSize],
                                    char guid[webrtc::kAdmMaxGuidSize]) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->PlayoutDeviceName(index, name, guid);
  }
  return -1;
}

int32_t AdmProxy::RecordingDeviceName(uint16_t index,
                                      char name[webrtc::kAdmMaxDeviceNameSize],
                                      char guid[webrtc::kAdmMaxGuidSize]) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->RecordingDeviceName(index, name, guid);
  }
  return -1;
}

int32_t AdmProxy::SetPlayoutDevice(uint16_t index) {
  webrtc::MutexLock lock(&mutex_);
  selected_playout_device_ = index;

  // Also store the GUID for this device for robust restoration
  if (platform_adm_) {
    char name[webrtc::kAdmMaxDeviceNameSize] = {0};
    char guid[webrtc::kAdmMaxGuidSize] = {0};
    if (platform_adm_->PlayoutDeviceName(index, name, guid) == 0) {
      selected_playout_guid_ = guid;
    }
    return platform_adm_->SetPlayoutDevice(index);
  }
  return 0;
}

int32_t AdmProxy::SetPlayoutDevice(WindowsDeviceType device) {
  webrtc::MutexLock lock(&mutex_);
  // Note: When using WindowsDeviceType, we can't easily get the GUID
  // The GUID will be populated on next CreatePlatformAdm if needed
  selected_playout_guid_.clear();
  if (platform_adm_) {
    return platform_adm_->SetPlayoutDevice(device);
  }
  return 0;
}

int32_t AdmProxy::SetRecordingDevice(uint16_t index) {
  webrtc::MutexLock lock(&mutex_);
  selected_recording_device_ = index;

  // Also store the GUID for this device for robust restoration
  if (platform_adm_) {
    char name[webrtc::kAdmMaxDeviceNameSize] = {0};
    char guid[webrtc::kAdmMaxGuidSize] = {0};
    if (platform_adm_->RecordingDeviceName(index, name, guid) == 0) {
      selected_recording_guid_ = guid;
    }
    return platform_adm_->SetRecordingDevice(index);
  }
  return 0;
}

int32_t AdmProxy::SetRecordingDevice(WindowsDeviceType device) {
  webrtc::MutexLock lock(&mutex_);
  // Note: When using WindowsDeviceType, we can't easily get the GUID
  // The GUID will be populated on next CreatePlatformAdm if needed
  selected_recording_guid_.clear();
  if (platform_adm_) {
    return platform_adm_->SetRecordingDevice(device);
  }
  return 0;
}

int32_t AdmProxy::PlayoutIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->PlayoutIsAvailable(available);
  }
  *available = true;  // Synthetic playout is always available
  return 0;
}

int32_t AdmProxy::InitPlayout() {
  webrtc::MutexLock lock(&mutex_);

  if (is_platform_playout_active()) {
    if (platform_adm_) {
      int32_t result = platform_adm_->InitPlayout();
      if (result == 0) {
        playout_initialized_ = true;
      }
      return result;
    }
    return -1;
  }

  // Synthetic mode
  if (synthetic_adm_) {
    int32_t result = synthetic_adm_->InitPlayout();
    if (result == 0) {
      playout_initialized_ = true;
    }
    return result;
  }
  return -1;
}

bool AdmProxy::PlayoutIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (is_platform_playout_active()) {
    return platform_adm_ && platform_adm_->PlayoutIsInitialized();
  }
  // Synthetic mode
  return synthetic_adm_ && synthetic_adm_->PlayoutIsInitialized();
}

int32_t AdmProxy::RecordingIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->RecordingIsAvailable(available);
  }
  *available = false;  // Recording not available in synthetic mode
  return 0;
}

int32_t AdmProxy::InitRecording() {
  webrtc::MutexLock lock(&mutex_);

  auto* adm = recording_adm();
  if (!adm) {
    // Recording not available (no platform ADM or recording disabled)
    // Return success to avoid breaking WebRTC's initialization flow
    return 0;
  }

  int32_t result = adm->InitRecording();
  if (result == 0) {
    recording_initialized_ = true;
  }
  return result;
}

bool AdmProxy::RecordingIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  auto* adm = recording_adm();
  if (adm) {
    return adm->RecordingIsInitialized();
  }
  return false;  // Recording not available
}

int32_t AdmProxy::StartPlayout() {
  webrtc::MutexLock lock(&mutex_);
  playing_ = true;

  if (is_platform_playout_active()) {
    if (platform_adm_) {
      return platform_adm_->StartPlayout();
    }
    return -1;
  }

  // Synthetic mode
  if (synthetic_adm_) {
    return synthetic_adm_->StartPlayout();
  }
  return -1;
}

int32_t AdmProxy::StopPlayout() {
  webrtc::MutexLock lock(&mutex_);
  playing_ = false;

  // Stop both ADMs
  if (synthetic_adm_) {
    synthetic_adm_->StopPlayout();
  }
  if (platform_adm_) {
    platform_adm_->StopPlayout();
  }
  return 0;
}

bool AdmProxy::Playing() const {
  webrtc::MutexLock lock(&mutex_);
  if (is_platform_playout_active()) {
    return platform_adm_ && platform_adm_->Playing();
  }
  return synthetic_adm_ && synthetic_adm_->Playing();
}

int32_t AdmProxy::StartRecording() {
  webrtc::MutexLock lock(&mutex_);

  auto* adm = recording_adm();
  if (!adm) {
    // Recording not available - return success to avoid breaking WebRTC
    return 0;
  }

  recording_ = true;
  return adm->StartRecording();
}

int32_t AdmProxy::StopRecording() {
  webrtc::MutexLock lock(&mutex_);
  recording_ = false;

  auto* adm = recording_adm();
  if (adm) {
    return adm->StopRecording();
  }
  return 0;
}

bool AdmProxy::Recording() const {
  webrtc::MutexLock lock(&mutex_);
  auto* adm = recording_adm();
  if (adm) {
    return adm->Recording();
  }
  return false;
}

int32_t AdmProxy::InitSpeaker() {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->InitSpeaker();
  }
  return 0;
}

bool AdmProxy::SpeakerIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SpeakerIsInitialized();
  }
  return true;
}

int32_t AdmProxy::InitMicrophone() {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->InitMicrophone();
  }
  return 0;
}

bool AdmProxy::MicrophoneIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MicrophoneIsInitialized();
  }
  return false;
}

int32_t AdmProxy::SpeakerVolumeIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SpeakerVolumeIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetSpeakerVolume(uint32_t volume) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetSpeakerVolume(volume);
  }
  return -1;
}

int32_t AdmProxy::SpeakerVolume(uint32_t* volume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SpeakerVolume(volume);
  }
  return -1;
}

int32_t AdmProxy::MaxSpeakerVolume(uint32_t* maxVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MaxSpeakerVolume(maxVolume);
  }
  return -1;
}

int32_t AdmProxy::MinSpeakerVolume(uint32_t* minVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MinSpeakerVolume(minVolume);
  }
  return -1;
}

int32_t AdmProxy::MicrophoneVolumeIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MicrophoneVolumeIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetMicrophoneVolume(uint32_t volume) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetMicrophoneVolume(volume);
  }
  return -1;
}

int32_t AdmProxy::MicrophoneVolume(uint32_t* volume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MicrophoneVolume(volume);
  }
  return -1;
}

int32_t AdmProxy::MaxMicrophoneVolume(uint32_t* maxVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MaxMicrophoneVolume(maxVolume);
  }
  return -1;
}

int32_t AdmProxy::MinMicrophoneVolume(uint32_t* minVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MinMicrophoneVolume(minVolume);
  }
  return -1;
}

int32_t AdmProxy::SpeakerMuteIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SpeakerMuteIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetSpeakerMute(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetSpeakerMute(enable);
  }
  return -1;
}

int32_t AdmProxy::SpeakerMute(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SpeakerMute(enabled);
  }
  return -1;
}

int32_t AdmProxy::MicrophoneMuteIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MicrophoneMuteIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetMicrophoneMute(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetMicrophoneMute(enable);
  }
  return -1;
}

int32_t AdmProxy::MicrophoneMute(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MicrophoneMute(enabled);
  }
  return -1;
}

int32_t AdmProxy::StereoPlayoutIsAvailable(bool* available) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->StereoPlayoutIsAvailable(available);
  }
  *available = true;
  return 0;
}

int32_t AdmProxy::SetStereoPlayout(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetStereoPlayout(enable);
  }
  return 0;
}

int32_t AdmProxy::StereoPlayout(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->StereoPlayout(enabled);
  }
  *enabled = true;
  return 0;
}

int32_t AdmProxy::StereoRecordingIsAvailable(bool* available) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->StereoRecordingIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetStereoRecording(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetStereoRecording(enable);
  }
  return 0;
}

int32_t AdmProxy::StereoRecording(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->StereoRecording(enabled);
  }
  *enabled = false;
  return 0;
}

int32_t AdmProxy::PlayoutDelay(uint16_t* delayMS) const {
  webrtc::MutexLock lock(&mutex_);
  if (is_platform_playout_active()) {
    if (platform_adm_) {
      return platform_adm_->PlayoutDelay(delayMS);
    }
  } else if (synthetic_adm_) {
    return synthetic_adm_->PlayoutDelay(delayMS);
  }
  *delayMS = 0;
  return 0;
}

bool AdmProxy::BuiltInAECIsAvailable() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->BuiltInAECIsAvailable();
  }
  return false;
}

bool AdmProxy::BuiltInAGCIsAvailable() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->BuiltInAGCIsAvailable();
  }
  return false;
}

bool AdmProxy::BuiltInNSIsAvailable() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->BuiltInNSIsAvailable();
  }
  return false;
}

int32_t AdmProxy::EnableBuiltInAEC(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->EnableBuiltInAEC(enable);
  }
  return -1;
}

int32_t AdmProxy::EnableBuiltInAGC(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->EnableBuiltInAGC(enable);
  }
  return -1;
}

int32_t AdmProxy::EnableBuiltInNS(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->EnableBuiltInNS(enable);
  }
  return -1;
}

#if defined(WEBRTC_IOS)
int AdmProxy::GetPlayoutAudioParameters(webrtc::AudioParameters* params) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->GetPlayoutAudioParameters(params);
  }
  return -1;
}

int AdmProxy::GetRecordAudioParameters(webrtc::AudioParameters* params) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->GetRecordAudioParameters(params);
  }
  return -1;
}
#endif

int32_t AdmProxy::SetObserver(webrtc::AudioDeviceObserver* observer) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetObserver(observer);
  }
  return 0;
}

}  // namespace livekit_ffi
