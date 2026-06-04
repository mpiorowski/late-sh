/*
 * Copyright 2025 LiveKit, Inc.
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

#include "livekit/synthetic_audio_device.h"

const int kSampleRate = 48000;
const int kChannels = 2;
const int kBytesPerSample = kChannels * sizeof(int16_t);
const int kSamplesPer10Ms = kSampleRate / 100;

namespace livekit_ffi {

SyntheticAudioDevice::SyntheticAudioDevice(const webrtc::Environment& env)
    : env_(env),
      data_(kSamplesPer10Ms * kChannels) {}

SyntheticAudioDevice::~SyntheticAudioDevice() {
  Terminate();
}

int32_t SyntheticAudioDevice::ActiveAudioLayer(AudioLayer* audioLayer) const {
  *audioLayer = AudioLayer::kDummyAudio;
  return 0;
}

int32_t SyntheticAudioDevice::RegisterAudioCallback(webrtc::AudioTransport* transport) {
  webrtc::MutexLock lock(&mutex_);
  audio_transport_ = transport;
  return 0;
}

int32_t SyntheticAudioDevice::Init() {
  webrtc::MutexLock lock(&mutex_);
  if (initialized_)
    return 0;

  audio_queue_ =
      env_.task_queue_factory().CreateTaskQueue(
          "SyntheticAudioDevice", webrtc::TaskQueueFactory::Priority::NORMAL);

  audio_task_ =
      webrtc::RepeatingTaskHandle::Start(audio_queue_.get(), [this]() {
        webrtc::MutexLock lock(&mutex_);

        if (playing_) {
          int64_t elapsed_time_ms = -1;
          int64_t ntp_time_ms = -1;
          size_t n_samples_out = 0;
          void* data = data_.data();

          // Request the AudioData, otherwise WebRTC will ignore the packets.
          // 10ms of audio data.
          audio_transport_->NeedMorePlayData(
              kSamplesPer10Ms, kBytesPerSample, kChannels, kSampleRate, data,
              n_samples_out, &elapsed_time_ms, &ntp_time_ms);
        }

        return webrtc::TimeDelta::Millis(10);
      });

  initialized_ = true;
  return 0;
}

int32_t SyntheticAudioDevice::Terminate() {
  {
    webrtc::MutexLock lock(&mutex_);
    if (!initialized_)
      return 0;

    initialized_ = false;
  }
  audio_queue_ = nullptr;
  return 0;
}

bool SyntheticAudioDevice::Initialized() const {
  webrtc::MutexLock lock(&mutex_);
  return initialized_;
}

int16_t SyntheticAudioDevice::PlayoutDevices() {
  return 0;
}

int16_t SyntheticAudioDevice::RecordingDevices() {
  return 0;
}

int32_t SyntheticAudioDevice::PlayoutDeviceName(uint16_t index,
                                       char name[webrtc::kAdmMaxDeviceNameSize],
                                       char guid[webrtc::kAdmMaxGuidSize]) {
  return 0;
}

int32_t SyntheticAudioDevice::RecordingDeviceName(
    uint16_t index,
    char name[webrtc::kAdmMaxDeviceNameSize],
    char guid[webrtc::kAdmMaxGuidSize]) {
  return 0;
}

int32_t SyntheticAudioDevice::SetPlayoutDevice(uint16_t index) {
  return 0;
}

int32_t SyntheticAudioDevice::SetPlayoutDevice(WindowsDeviceType device) {
  return 0;
}

int32_t SyntheticAudioDevice::SetRecordingDevice(uint16_t index) {
  return 0;
}

int32_t SyntheticAudioDevice::SetRecordingDevice(WindowsDeviceType device) {
  return 0;
}

int32_t SyntheticAudioDevice::PlayoutIsAvailable(bool* available) {
  return 0;
}

int32_t SyntheticAudioDevice::InitPlayout() {
  return 0;
}

bool SyntheticAudioDevice::PlayoutIsInitialized() const {
  return false;
}

int32_t SyntheticAudioDevice::RecordingIsAvailable(bool* available) {
  return 0;
}

int32_t SyntheticAudioDevice::InitRecording() {
  return 0;
}

bool SyntheticAudioDevice::RecordingIsInitialized() const {
  return false;
}

int32_t SyntheticAudioDevice::StartPlayout() {
  webrtc::MutexLock lock(&mutex_);
  playing_ = true;
  return 0;
}

int32_t SyntheticAudioDevice::StopPlayout() {
  webrtc::MutexLock lock(&mutex_);
  playing_ = false;
  return 0;
}

bool SyntheticAudioDevice::Playing() const {
  webrtc::MutexLock lock(&mutex_);
  return playing_;
}

int32_t SyntheticAudioDevice::StartRecording() {
  return 0;
}

int32_t SyntheticAudioDevice::StopRecording() {
  return 0;
}

bool SyntheticAudioDevice::Recording() const {
  return false;
}

int32_t SyntheticAudioDevice::InitSpeaker() {
  return 0;
}

bool SyntheticAudioDevice::SpeakerIsInitialized() const {
  return false;
}

int32_t SyntheticAudioDevice::InitMicrophone() {
  return 0;
}

bool SyntheticAudioDevice::MicrophoneIsInitialized() const {
  return false;
}

int32_t SyntheticAudioDevice::SpeakerVolumeIsAvailable(bool* available) {
  return 0;
}

int32_t SyntheticAudioDevice::SetSpeakerVolume(uint32_t volume) {
  return 0;
}

int32_t SyntheticAudioDevice::SpeakerVolume(uint32_t* volume) const {
  return 0;
}

int32_t SyntheticAudioDevice::MaxSpeakerVolume(uint32_t* maxVolume) const {
  return 0;
}

int32_t SyntheticAudioDevice::MinSpeakerVolume(uint32_t* minVolume) const {
  return 0;
}

int32_t SyntheticAudioDevice::MicrophoneVolumeIsAvailable(bool* available) {
  return 0;
}

int32_t SyntheticAudioDevice::SetMicrophoneVolume(uint32_t volume) {
  return 0;
}

int32_t SyntheticAudioDevice::MicrophoneVolume(uint32_t* volume) const {
  return 0;
}

int32_t SyntheticAudioDevice::MaxMicrophoneVolume(uint32_t* maxVolume) const {
  return 0;
}

int32_t SyntheticAudioDevice::MinMicrophoneVolume(uint32_t* minVolume) const {
  return 0;
}

int32_t SyntheticAudioDevice::SpeakerMuteIsAvailable(bool* available) {
  return 0;
}

int32_t SyntheticAudioDevice::SetSpeakerMute(bool enable) {
  return 0;
}

int32_t SyntheticAudioDevice::SpeakerMute(bool* enabled) const {
  return 0;
}

int32_t SyntheticAudioDevice::MicrophoneMuteIsAvailable(bool* available) {
  return 0;
}

int32_t SyntheticAudioDevice::SetMicrophoneMute(bool enable) {
  return 0;
}

int32_t SyntheticAudioDevice::MicrophoneMute(bool* enabled) const {
  return 0;
}

int32_t SyntheticAudioDevice::StereoPlayoutIsAvailable(bool* available) const {
  *available = true;
  return 0;
}

int32_t SyntheticAudioDevice::SetStereoPlayout(bool enable) {
  return 0;
}

int32_t SyntheticAudioDevice::StereoPlayout(bool* enabled) const {
  return 0;
}

int32_t SyntheticAudioDevice::StereoRecordingIsAvailable(bool* available) const {
  return 0;
}

int32_t SyntheticAudioDevice::SetStereoRecording(bool enable) {
  return 0;
}

int32_t SyntheticAudioDevice::StereoRecording(bool* enabled) const {
  *enabled = true;
  return 0;
}

int32_t SyntheticAudioDevice::PlayoutDelay(uint16_t* delayMS) const {
  return 0;
}

bool SyntheticAudioDevice::BuiltInAECIsAvailable() const {
  return false;
}

bool SyntheticAudioDevice::BuiltInAGCIsAvailable() const {
  return false;
}

bool SyntheticAudioDevice::BuiltInNSIsAvailable() const {
  return false;
}

int32_t SyntheticAudioDevice::EnableBuiltInAEC(bool enable) {
  return 0;
}

int32_t SyntheticAudioDevice::EnableBuiltInAGC(bool enable) {
  return 0;
}

int32_t SyntheticAudioDevice::EnableBuiltInNS(bool enable) {
  return 0;
}

#if defined(WEBRTC_IOS)
int SyntheticAudioDevice::GetPlayoutAudioParameters(
    webrtc::AudioParameters* params) const {
  return 0;
}

int SyntheticAudioDevice::GetRecordAudioParameters(
    webrtc::AudioParameters* params) const {
  return 0;
}
#endif  // WEBRTC_IOS

int32_t SyntheticAudioDevice::SetObserver(webrtc::AudioDeviceObserver* observer) {
  return 0;
}

}  // namespace livekit_ffi
