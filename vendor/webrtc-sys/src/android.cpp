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

#include "livekit/android.h"

#include <atomic>
#include <jni.h>
#include <stdio.h>
#include <unistd.h>
#include <memory>

#include "api/video_codecs/video_decoder_factory.h"
#include "rtc_base/logging.h"
#include "sdk/android/native_api/base/init.h"
#include "sdk/android/native_api/codecs/wrapper.h"
#include "sdk/android/native_api/jni/class_loader.h"
#include "sdk/android/native_api/jni/scoped_java_ref.h"
#include "sdk/android/src/jni/jni_helpers.h"

// When compiling the examples app on Android, the linker complains that
// `stdout` and `stderr` symbols cannot be found. The previous workaround
// referenced `__sF`, but that symbol was removed in NDK 28+.
// Use POSIX `fdopen()` with the standard file descriptors instead — works
// across all NDK versions.
#undef stdout
FILE *stdout = fdopen(STDOUT_FILENO, "w");

#undef stderr
FILE *stderr = fdopen(STDERR_FILENO, "w");

namespace livekit_ffi {

// Track whether Android WebRTC has been initialized to prevent crashes on double-init.
static std::atomic<bool> g_android_initialized{false};

void init_android(JavaVM* jvm) {
  // Idempotent - safe to call multiple times
  if (g_android_initialized.exchange(true)) {
    RTC_LOG(LS_INFO) << "livekit_ffi::init_android() - already initialized, skipping";
    return;
  }

  RTC_LOG(LS_INFO) << "livekit_ffi::init_android() called with jvm=" << (jvm ? "valid" : "null");
  if (!jvm) {
    RTC_LOG(LS_ERROR) << "livekit_ffi::init_android() - JavaVM is null! Cannot initialize Android WebRTC.";
    g_android_initialized.store(false);
    return;
  }
  webrtc::InitAndroid(jvm);
  RTC_LOG(LS_INFO) << "livekit_ffi::init_android() - webrtc::InitAndroid() completed";
}

bool init_android_context(JavaVM* jvm, uintptr_t context_ptr) {
  RTC_LOG(LS_INFO) << "livekit_ffi::init_android_context() called";

  if (!jvm || !context_ptr) {
    RTC_LOG(LS_ERROR) << "livekit_ffi::init_android_context() - jvm or context is null";
    return false;
  }

  // Initialize JVM first (idempotent - WebRTC handles double-init internally)
  init_android(jvm);

  // Cast uintptr_t back to jobject
  jobject context = reinterpret_cast<jobject>(context_ptr);

  JNIEnv* env = webrtc::AttachCurrentThreadIfNeeded();
  if (!env) {
    RTC_LOG(LS_ERROR) << "livekit_ffi::init_android_context() - Failed to attach to JNI";
    return false;
  }

  // Find livekit.org.webrtc.ContextUtils class
  jclass context_utils_class = env->FindClass("livekit/org/webrtc/ContextUtils");
  if (!context_utils_class) {
    RTC_LOG(LS_ERROR) << "livekit_ffi::init_android_context() - Failed to find ContextUtils class";
    env->ExceptionClear();
    return false;
  }

  // Get the initialize method
  jmethodID initialize_method = env->GetStaticMethodID(
      context_utils_class, "initialize", "(Landroid/content/Context;)V");
  if (!initialize_method) {
    RTC_LOG(LS_ERROR) << "livekit_ffi::init_android_context() - Failed to find initialize method";
    env->ExceptionClear();
    env->DeleteLocalRef(context_utils_class);
    return false;
  }

  // Call ContextUtils.initialize(context)
  env->CallStaticVoidMethod(context_utils_class, initialize_method, context);

  // Check for exceptions
  if (env->ExceptionCheck()) {
    RTC_LOG(LS_ERROR) << "livekit_ffi::init_android_context() - Exception during initialize";
    env->ExceptionDescribe();
    env->ExceptionClear();
    env->DeleteLocalRef(context_utils_class);
    return false;
  }

  env->DeleteLocalRef(context_utils_class);
  RTC_LOG(LS_INFO) << "livekit_ffi::init_android_context() - ContextUtils initialized successfully";
  return true;
}

std::unique_ptr<webrtc::VideoEncoderFactory>
CreateAndroidVideoEncoderFactory() {
  JNIEnv* env = webrtc::AttachCurrentThreadIfNeeded();
  webrtc::ScopedJavaLocalRef<jclass> factory_class =
      webrtc::GetClass(env, "livekit/org/webrtc/DefaultVideoEncoderFactory");

  jmethodID ctor = env->GetMethodID(factory_class.obj(), "<init>",
                                    "(Llivekit/org/webrtc/EglBase$Context;ZZ)V");

  jobject encoder_factory =
      env->NewObject(factory_class.obj(), ctor, nullptr, true, false);

  return webrtc::JavaToNativeVideoEncoderFactory(env, encoder_factory);
}

std::unique_ptr<webrtc::VideoDecoderFactory>
CreateAndroidVideoDecoderFactory() {
  JNIEnv* env = webrtc::AttachCurrentThreadIfNeeded();

  webrtc::ScopedJavaLocalRef<jclass> factory_class =
      webrtc::GetClass(env, "livekit/org/webrtc/WrappedVideoDecoderFactory");

  jmethodID ctor = env->GetMethodID(factory_class.obj(), "<init>",
                                    "(Llivekit/org/webrtc/EglBase$Context;)V");

  jobject decoder_factory = env->NewObject(factory_class.obj(), ctor, nullptr);
  return webrtc::JavaToNativeVideoDecoderFactory(env, decoder_factory);
}

}  // namespace livekit_ffi
