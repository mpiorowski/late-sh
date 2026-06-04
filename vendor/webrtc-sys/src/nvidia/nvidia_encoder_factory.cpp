#include "nvidia_encoder_factory.h"

#include <memory>

#include "cuda_context.h"
#include "h264_encoder_impl.h"
#include "h265_encoder_impl.h"
#include "nvEncodeAPI.h"
#include "rtc_base/logging.h"

#include <dlfcn.h>

namespace webrtc {

NvidiaVideoEncoderFactory::NvidiaVideoEncoderFactory() {
  std::map<std::string, std::string> baselineParameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));

  // Advertise HEVC/H265 with default parameters.
  supported_formats_.push_back(SdpVideoFormat("H265"));
  // Some stacks use 'HEVC' name.
  supported_formats_.push_back(SdpVideoFormat("HEVC"));

  /*std::map<std::string, std::string> highParameters = {
      {"profile-level-id", "4d0032"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };

  supported_formats_.push_back(SdpVideoFormat("H264", highParameters));
  */
}

NvidiaVideoEncoderFactory::~NvidiaVideoEncoderFactory() {}

bool NvidiaVideoEncoderFactory::IsSupported() {
  if (!livekit_ffi::CudaContext::IsAvailable()) {
    RTC_LOG(LS_WARNING) << "CUDA is not available, NVENC disabled.";
    return false;
  }

  // CUDA being available does NOT imply NVENC is present. Compute-only GPUs
  // (H100, A100, etc.) have full CUDA support but no encode hardware.
  // Probe the NVENC library and try to open a session to be sure.
  void* hModule = dlopen("libnvidia-encode.so.1", RTLD_LAZY);
  if (!hModule) {
    RTC_LOG(LS_WARNING) << "NVENC library (libnvidia-encode.so.1) not found, "
                           "hardware encoding unavailable.";
    return false;
  }
  auto NvEncodeAPIGetMaxSupportedVersion =
      (NVENCSTATUS(NVENCAPI*)(uint32_t*))dlsym(
          hModule, "NvEncodeAPIGetMaxSupportedVersion");
  auto NvEncodeAPICreateInstance =
      (NVENCSTATUS(NVENCAPI*)(NV_ENCODE_API_FUNCTION_LIST*))dlsym(
          hModule, "NvEncodeAPICreateInstance");

  bool supported = false;

  do {
    if (!NvEncodeAPIGetMaxSupportedVersion || !NvEncodeAPICreateInstance) {
      RTC_LOG(LS_WARNING) << "NVENC API entry points not found in library.";
      break;
    }

    uint32_t maxVersion = 0;
    if (NvEncodeAPIGetMaxSupportedVersion(&maxVersion) != NV_ENC_SUCCESS) {
      RTC_LOG(LS_WARNING) << "NvEncodeAPIGetMaxSupportedVersion failed.";
      break;
    }

    uint32_t currentVersion =
        (NVENCAPI_MAJOR_VERSION << 4) | NVENCAPI_MINOR_VERSION;
    if (currentVersion > maxVersion) {
      RTC_LOG(LS_WARNING) << "NVENC driver version too old: driver supports "
                          << maxVersion << ", SDK requires " << currentVersion;
      break;
    }

    NV_ENCODE_API_FUNCTION_LIST fnList = {NV_ENCODE_API_FUNCTION_LIST_VER};
    if (NvEncodeAPICreateInstance(&fnList) != NV_ENC_SUCCESS) {
      RTC_LOG(LS_WARNING) << "NvEncodeAPICreateInstance failed.";
      break;
    }

    // Try opening an encode session with CUDA device 0 to confirm the GPU
    // actually has NVENC hardware.
    CUresult cuRes = cuInit(0);
    if (cuRes != CUDA_SUCCESS) {
      RTC_LOG(LS_WARNING) << "cuInit failed during NVENC probe.";
      break;
    }

    CUdevice cuDevice;
    cuRes = cuDeviceGet(&cuDevice, 0);
    if (cuRes != CUDA_SUCCESS) {
      RTC_LOG(LS_WARNING) << "cuDeviceGet failed during NVENC probe.";
      break;
    }

    CUcontext cuCtx = nullptr;
#if CUDA_VERSION >= 13000
    cuRes = cuCtxCreate(&cuCtx, nullptr, 0, cuDevice);
#else
    cuRes = cuCtxCreate(&cuCtx, 0, cuDevice);
#endif
    if (cuRes != CUDA_SUCCESS || !cuCtx) {
      RTC_LOG(LS_WARNING) << "cuCtxCreate failed during NVENC probe.";
      break;
    }

    NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS sessionParams = {
        NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER};
    sessionParams.device = cuCtx;
    sessionParams.deviceType = NV_ENC_DEVICE_TYPE_CUDA;
    sessionParams.apiVersion = NVENCAPI_VERSION;

    void* hEncoder = nullptr;
    NVENCSTATUS nvStatus =
        fnList.nvEncOpenEncodeSessionEx(&sessionParams, &hEncoder);

    if (nvStatus == NV_ENC_SUCCESS && hEncoder) {
      fnList.nvEncDestroyEncoder(hEncoder);
      supported = true;
    } else {
      char deviceName[80] = {};
      cuDeviceGetName(deviceName, sizeof(deviceName), cuDevice);
      RTC_LOG(LS_WARNING) << "NVENC not available on GPU \"" << deviceName
                          << "\" (status=" << nvStatus
                          << "). This GPU likely lacks encode hardware.";
    }

    cuCtxDestroy(cuCtx);
  } while (false);

  dlclose(hModule);

  if (supported) {
    RTC_LOG(LS_INFO) << "NVIDIA NVENC hardware encoder is available.";
  }
  return supported;
}

std::unique_ptr<VideoEncoder> NvidiaVideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  // Check if the requested format is supported.
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      if (!cu_context_) {
        cu_context_ = livekit_ffi::CudaContext::GetInstance();
        if (!cu_context_->Initialize()) {
          RTC_LOG(LS_ERROR) << "Failed to initialize CUDA context.";
          return nullptr;
        }
      }

      if (format.name == "H264") {
        RTC_LOG(LS_INFO) << "Using NVIDIA HW encoder (NVENC) for H264";
        return std::make_unique<NvidiaH264EncoderImpl>(
            env, cu_context_->GetContext(), CU_MEMORYTYPE_DEVICE,
            NV_ENC_BUFFER_FORMAT_IYUV, format);
      }

      if (format.name == "H265" || format.name == "HEVC") {
        RTC_LOG(LS_INFO) << "Using NVIDIA HW encoder (NVENC) for H265/HEVC";
        return std::make_unique<NvidiaH265EncoderImpl>(
            env, cu_context_->GetContext(), CU_MEMORYTYPE_DEVICE,
            NV_ENC_BUFFER_FORMAT_IYUV, format);
      }
    }
  }
  return nullptr;
}
std::vector<SdpVideoFormat> NvidiaVideoEncoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

std::vector<SdpVideoFormat> NvidiaVideoEncoderFactory::GetImplementations()
    const {
  return supported_formats_;
}

}  // namespace webrtc
