// Copyright 2025 LiveKit, Inc.
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

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/apm.h");

        type AudioProcessingModule;

        unsafe fn process_stream(
            self: Pin<&mut AudioProcessingModule>,
            src: *const i16,
            src_len: usize,
            dst: *mut i16,
            dst_len: usize,
            sample_rate: i32,
            num_channels: i32,
        ) -> i32;

        unsafe fn process_reverse_stream(
            self: Pin<&mut AudioProcessingModule>,
            src: *const i16,
            src_len: usize,
            dst: *mut i16,
            dst_len: usize,
            sample_rate: i32,
            num_channels: i32,
        ) -> i32;

        fn set_stream_delay_ms(self: Pin<&mut AudioProcessingModule>, delay: i32) -> i32;

        fn create_apm(
            echo_canceller_enabled: bool,
            gain_controller_enabled: bool,
            high_pass_filter_enabled: bool,
            noise_suppression_enabled: bool,
        ) -> UniquePtr<AudioProcessingModule>;
    }
}

impl_thread_safety!(ffi::AudioProcessingModule, Send + Sync);
