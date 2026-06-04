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

#pragma once

#include <jni.h>

#include <cstdint>
#include <memory>

#include "api/video_codecs/video_decoder_factory.h"
#include "api/video_codecs/video_encoder_factory.h"

namespace livekit_ffi {
typedef JavaVM JavaVM;
}  // namespace livekit_ffi
#include "webrtc-sys/src/android.rs.h"

namespace livekit_ffi {

/// Initialize Android WebRTC with the JVM.
/// This is called automatically by init_android_context(), so you only need to
/// call this directly in JNI_OnLoad or if you don't have an Android Context.
/// This function is idempotent - safe to call multiple times.
///
/// @param jvm The JavaVM pointer
void init_android(JavaVM* jvm);

/// Initialize Android WebRTC with the application context.
/// This is the main initialization function - it calls init_android() internally
/// and then initializes ContextUtils for PlatformAudio support.
/// This function is idempotent - safe to call multiple times.
///
/// @param jvm The JavaVM pointer
/// @param context The Android application context (jobject cast to uintptr_t)
/// @return true if context initialization was successful, false otherwise.
///         Note: JVM init (init_android) always happens regardless of return value.
bool init_android_context(JavaVM* jvm, uintptr_t context);

std::unique_ptr<webrtc::VideoEncoderFactory> CreateAndroidVideoEncoderFactory();
std::unique_ptr<webrtc::VideoDecoderFactory> CreateAndroidVideoDecoderFactory();

}  // namespace livekit_ffi
