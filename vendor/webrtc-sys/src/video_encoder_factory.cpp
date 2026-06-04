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

#include "livekit/video_encoder_factory.h"

#include <cstdlib>
#include <string_view>

#include "api/environment/environment_factory.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory_template.h"
#include "livekit/objc_video_factory.h"
#include "media/base/media_constants.h"
#include "media/engine/simulcast_encoder_adapter.h"
#include "rtc_base/logging.h"
#if defined(RTC_USE_LIBAOM_AV1_ENCODER)
#include "api/video_codecs/video_encoder_factory_template_libaom_av1_adapter.h"
#endif
#if defined(WEBRTC_USE_H264)
#include "api/video_codecs/video_encoder_factory_template_open_h264_adapter.h"
#endif
#include "api/video_codecs/video_encoder_factory_template_libvpx_vp8_adapter.h"
#include "api/video_codecs/video_encoder_factory_template_libvpx_vp9_adapter.h"

#ifdef WEBRTC_ANDROID
#include "livekit/android.h"
#endif

#if defined(USE_NVIDIA_VIDEO_CODEC)
#include "nvidia/nvidia_encoder_factory.h"
#endif

#if defined(USE_VAAPI_VIDEO_CODEC)
#include "vaapi/vaapi_encoder_factory.h"
#endif

namespace livekit_ffi {

namespace {

constexpr char kPreferredHwEncoderEnv[] = "LIVEKIT_PREFERRED_HW_ENCODER";

enum class PreferredHwEncoder {
  kNvenc,
  kVaapi,
};

struct PreferredHwEncoderConfig {
  PreferredHwEncoder encoder = PreferredHwEncoder::kNvenc;
  bool explicitly_set = false;
};

PreferredHwEncoderConfig GetPreferredHwEncoderConfig() {
  const char* preferred_encoder = std::getenv(kPreferredHwEncoderEnv);
  if (!preferred_encoder) {
    return {};
  }

  std::string_view preferred_encoder_view(preferred_encoder);
  if (preferred_encoder_view == "nvenc") {
    return {PreferredHwEncoder::kNvenc, true};
  }
  if (preferred_encoder_view == "vaapi") {
    return {PreferredHwEncoder::kVaapi, true};
  }

  RTC_LOG(LS_WARNING) << "Ignoring invalid LIVEKIT_PREFERRED_HW_ENCODER=\""
                      << preferred_encoder
                      << "\"; expected \"nvenc\" or \"vaapi\".";
  return {};
}

void AddNvencFactory(
    std::vector<std::unique_ptr<webrtc::VideoEncoderFactory>>& factories,
    bool preferred) {
#if defined(USE_NVIDIA_VIDEO_CODEC)
  if (webrtc::NvidiaVideoEncoderFactory::IsSupported()) {
    factories.push_back(std::make_unique<webrtc::NvidiaVideoEncoderFactory>());
    return;
  }

  if (preferred) {
    RTC_LOG(LS_WARNING)
        << "LIVEKIT_PREFERRED_HW_ENCODER=nvenc requested, but NVENC "
           "is unavailable; falling back to other encoders.";
  }
#else
  if (preferred) {
    RTC_LOG(LS_WARNING)
        << "LIVEKIT_PREFERRED_HW_ENCODER=nvenc requested, but NVENC support "
           "is not compiled in; falling back to other encoders.";
  }
#endif
}

void AddVaapiFactory(
    std::vector<std::unique_ptr<webrtc::VideoEncoderFactory>>& factories,
    bool preferred) {
#if defined(USE_VAAPI_VIDEO_CODEC)
  if (webrtc::VAAPIVideoEncoderFactory::IsSupported()) {
    factories.push_back(std::make_unique<webrtc::VAAPIVideoEncoderFactory>());
    return;
  }

  if (preferred) {
    RTC_LOG(LS_WARNING)
        << "LIVEKIT_PREFERRED_HW_ENCODER=vaapi requested, but VAAPI "
           "is unavailable; falling back to other encoders.";
  }
#else
  if (preferred) {
    RTC_LOG(LS_WARNING)
        << "LIVEKIT_PREFERRED_HW_ENCODER=vaapi requested, but VAAPI support "
           "is not compiled in; falling back to other encoders.";
  }
#endif
}

}  // namespace

using Factory = webrtc::VideoEncoderFactoryTemplate<
    webrtc::LibvpxVp8EncoderTemplateAdapter,
#if defined(WEBRTC_USE_H264)
    webrtc::OpenH264EncoderTemplateAdapter,
#endif
#if defined(RTC_USE_LIBAOM_AV1_ENCODER)
    webrtc::LibaomAv1EncoderTemplateAdapter,
#endif
    webrtc::LibvpxVp9EncoderTemplateAdapter>;

VideoEncoderFactory::InternalFactory::InternalFactory() {
#if defined(__APPLE__) && !defined(LIVEKIT_DISABLE_MACOS_OBJC_VIDEO_FACTORY)
  factories_.push_back(livekit_ffi::CreateObjCVideoEncoderFactory());
#endif

#ifdef WEBRTC_ANDROID
  factories_.push_back(CreateAndroidVideoEncoderFactory());
#endif

  const PreferredHwEncoderConfig preferred_hw_encoder =
      GetPreferredHwEncoderConfig();
  if (preferred_hw_encoder.encoder == PreferredHwEncoder::kVaapi) {
    AddVaapiFactory(factories_, preferred_hw_encoder.explicitly_set);
    AddNvencFactory(factories_, false);
  } else {
    AddNvencFactory(factories_, preferred_hw_encoder.explicitly_set);
    AddVaapiFactory(factories_, false);
  }
}

std::vector<webrtc::SdpVideoFormat>
VideoEncoderFactory::InternalFactory::GetSupportedFormats() const {
  std::vector<webrtc::SdpVideoFormat> formats = Factory().GetSupportedFormats();

  for (const auto& factory : factories_) {
    auto supported_formats = factory->GetSupportedFormats();
    formats.insert(formats.end(), supported_formats.begin(),
                   supported_formats.end());
  }
  return formats;
}

VideoEncoderFactory::CodecSupport
VideoEncoderFactory::InternalFactory::QueryCodecSupport(
    const webrtc::SdpVideoFormat& format,
    std::optional<std::string> scalability_mode) const {
  auto original_format =
      webrtc::FuzzyMatchSdpVideoFormat(Factory().GetSupportedFormats(), format);
  return original_format
             ? Factory().QueryCodecSupport(*original_format, scalability_mode)
             : webrtc::VideoEncoderFactory::CodecSupport{.is_supported = false};
}

std::unique_ptr<webrtc::VideoEncoder>
VideoEncoderFactory::InternalFactory::Create(
    const webrtc::Environment& env,
    const webrtc::SdpVideoFormat& format) {
  for (const auto& factory : factories_) {
    for (const auto& supported_format : factory->GetSupportedFormats()) {
      if (supported_format.IsSameCodec(format))
        return factory->Create(env, format);
    }
  }

  auto original_format =
      webrtc::FuzzyMatchSdpVideoFormat(Factory().GetSupportedFormats(), format);

  if (original_format) {
    return Factory().Create(env, *original_format);
  }

  RTC_LOG(LS_ERROR) << "No VideoEncoder found for " << format.name;
  return nullptr;
}

VideoEncoderFactory::VideoEncoderFactory() {
  internal_factory_ = std::make_unique<InternalFactory>();
}

std::vector<webrtc::SdpVideoFormat> VideoEncoderFactory::GetSupportedFormats()
    const {
  return internal_factory_->GetSupportedFormats();
}

VideoEncoderFactory::CodecSupport VideoEncoderFactory::QueryCodecSupport(
    const webrtc::SdpVideoFormat& format,
    std::optional<std::string> scalability_mode) const {
  return internal_factory_->QueryCodecSupport(format, scalability_mode);
}

std::unique_ptr<webrtc::VideoEncoder> VideoEncoderFactory::Create(
    const webrtc::Environment& env,
    const webrtc::SdpVideoFormat& format) {
  std::unique_ptr<webrtc::VideoEncoder> encoder;
  if (format.IsCodecInList(internal_factory_->GetSupportedFormats())) {
    encoder = std::make_unique<webrtc::SimulcastEncoderAdapter>(
        env, internal_factory_.get(), nullptr, format);
  }

  return encoder;
}

}  // namespace livekit_ffi
