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

use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    #[derive(Debug)]
    #[repr(i32)]
    pub enum SdpType {
        Offer,
        PrAnswer,
        Answer,
        Rollback,
    }

    #[derive(Debug)]
    pub struct SdpParseError {
        pub line: String,
        pub description: String,
    }

    extern "C++" {
        include!("livekit/rtc_error.h");

        type RtcError = crate::rtc_error::ffi::RtcError;
    }

    unsafe extern "C++" {
        include!("livekit/jsep.h");

        type IceCandidate;
        type SessionDescription;

        fn sdp_mid(self: &IceCandidate) -> String;
        fn sdp_mline_index(self: &IceCandidate) -> i32;
        fn candidate(self: &IceCandidate) -> String;
        fn stringify(self: &IceCandidate) -> String;

        fn sdp_type(self: &SessionDescription) -> SdpType;
        fn stringify(self: &SessionDescription) -> String;
        fn clone(self: &SessionDescription) -> UniquePtr<SessionDescription>;

        fn create_ice_candidate(
            sdp_mid: String,
            sdp_mline_index: i32,
            sdp: String,
        ) -> Result<SharedPtr<IceCandidate>>;

        fn create_session_description(
            sdp_type: SdpType,
            sdp: String,
        ) -> Result<UniquePtr<SessionDescription>>;

        fn _shared_ice_candidate() -> SharedPtr<IceCandidate>; // Ignore
        fn _unique_session_description() -> UniquePtr<SessionDescription>; // Ignore
    }
}

impl Error for ffi::SdpParseError {}

impl Display for ffi::SdpParseError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "SdpParseError occurred {}: {}", self.line, self.description)
    }
}

impl_thread_safety!(ffi::SessionDescription, Send + Sync);
impl_thread_safety!(ffi::IceCandidate, Send + Sync);

impl ffi::SdpParseError {
    /// # Safety
    /// The value must be correctly encoded
    pub unsafe fn from(value: &str) -> Self {
        // Parse the hex encoded error from c++
        let line_length = u32::from_str_radix(&value[0..8], 16).unwrap() as usize + 8;
        let line = String::from(&value[8..line_length]);
        let description = String::from(&value[line_length..]);

        Self { line, description }
    }
}

#[cfg(test)]
mod tests {
    #[cxx::bridge(namespace = "livekit_ffi")]
    pub mod ffi_tests {
        unsafe extern "C++" {
            include!("livekit/jsep.h");

            fn serialize_sdp_parse_error_for_test() -> String;
        }
    }

    use crate::jsep::ffi;

    /// Tests that SdpParseError can correctly deserialize the hex-encoded
    /// error format produced by C++ when SDP parsing fails.
    #[test]
    fn sdp_parse_error_deserialization() {
        let serialized = ffi_tests::serialize_sdp_parse_error_for_test();
        let err = unsafe { ffi::SdpParseError::from(&serialized) };

        assert!(!err.line.is_empty(), "error line should not be empty");
        assert!(!err.description.is_empty(), "error description should not be empty");
    }
}
