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

// cxx doesn't support custom Exception type, so we serialize RtcError inside the cxx::Exception
// "what" string

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    #[derive(Debug)]
    #[repr(i32)]
    pub enum RtcErrorType {
        None,
        UnsupportedOperation,
        UnsupportedParameter,
        InvalidParameter,
        InvalidRange,
        SyntaxError,
        InvalidState,
        InvalidModification,
        NetworkError,
        ResourceExhausted,
        InternalError,
        OperationErrorWithData,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum RtcErrorDetailType {
        None,
        DataChannelFailure,
        DtlsFailure,
        FingerprintFailure,
        SctpFailure,
        SdpSyntaxError,
        HardwareEncoderNotAvailable,
        HardwareEncoderError,
    }

    #[derive(Debug)]
    pub struct RtcError {
        pub error_type: RtcErrorType,
        pub message: String,
        pub error_detail: RtcErrorDetailType,
        // cxx doesn't support the Option trait
        pub has_sctp_cause_code: bool,
        pub sctp_cause_code: u16,
    }
}

impl ffi::RtcError {
    /// Parse the hex-encoded error string the C++ side stuffs into the
    /// `cxx::Exception` "what" message (see `webrtc-sys/src/rtc_error.cpp`
    /// `serialize_error`). The format is fixed-width:
    ///
    /// ```text
    ///   bytes 0..8   error_type           (u32 hex)
    ///   bytes 8..16  error_detail         (u32 hex)
    ///   bytes 16..18 has_sctp_cause_code  (u8 hex, 0 or 1)
    ///   bytes 18..22 sctp_cause_code      (u16 hex)
    ///   bytes 22..   message              (raw, not encoded)
    /// ```
    ///
    /// Returns `None` if the input is shorter than the fixed header or the
    /// header bytes aren't valid hex. Discriminants outside the known
    /// variants for `RtcErrorType` / `RtcErrorDetailType` fall back to
    /// `None` for the affected field instead of being `transmute`d into
    /// the enum (which is instant UB and what nightly's
    /// `ptr::copy_nonoverlapping` precondition check was firing on).
    pub fn parse(value: &str) -> Option<Self> {
        if value.len() < 22 {
            return None;
        }
        let error_type = u32::from_str_radix(&value[0..8], 16).ok()?;
        let error_detail = u32::from_str_radix(&value[8..16], 16).ok()?;
        let has_scp_cause_code = u8::from_str_radix(&value[16..18], 16).ok()?;
        let sctp_cause_code = u16::from_str_radix(&value[18..22], 16).ok()?;
        let message = String::from(&value[22..]);

        Some(Self {
            error_type: rtc_error_type_from_u32(error_type),
            error_detail: rtc_error_detail_type_from_u32(error_detail),
            sctp_cause_code,
            has_sctp_cause_code: has_scp_cause_code == 1,
            message,
        })
    }

    /// Backwards-compatible wrapper for callers that already trust the input
    /// is well-formed.
    ///
    /// # Safety
    /// Marked `unsafe` purely for source-compat with prior callers; the body
    /// no longer relies on caller-upheld invariants.
    pub unsafe fn from(value: &str) -> Self {
        Self::parse(value).expect("malformed serialized RtcError")
    }

    pub fn ok(&self) -> bool {
        self.error_type == ffi::RtcErrorType::None
    }
}

fn rtc_error_type_from_u32(value: u32) -> ffi::RtcErrorType {
    match value {
        0 => ffi::RtcErrorType::None,
        1 => ffi::RtcErrorType::UnsupportedOperation,
        2 => ffi::RtcErrorType::UnsupportedParameter,
        3 => ffi::RtcErrorType::InvalidParameter,
        4 => ffi::RtcErrorType::InvalidRange,
        5 => ffi::RtcErrorType::SyntaxError,
        6 => ffi::RtcErrorType::InvalidState,
        7 => ffi::RtcErrorType::InvalidModification,
        8 => ffi::RtcErrorType::NetworkError,
        9 => ffi::RtcErrorType::ResourceExhausted,
        10 => ffi::RtcErrorType::InternalError,
        11 => ffi::RtcErrorType::OperationErrorWithData,
        _ => ffi::RtcErrorType::None,
    }
}

fn rtc_error_detail_type_from_u32(value: u32) -> ffi::RtcErrorDetailType {
    match value {
        0 => ffi::RtcErrorDetailType::None,
        1 => ffi::RtcErrorDetailType::DataChannelFailure,
        2 => ffi::RtcErrorDetailType::DtlsFailure,
        3 => ffi::RtcErrorDetailType::FingerprintFailure,
        4 => ffi::RtcErrorDetailType::SctpFailure,
        5 => ffi::RtcErrorDetailType::SdpSyntaxError,
        6 => ffi::RtcErrorDetailType::HardwareEncoderNotAvailable,
        7 => ffi::RtcErrorDetailType::HardwareEncoderError,
        _ => ffi::RtcErrorDetailType::None,
    }
}

impl Error for ffi::RtcError {}

impl Display for ffi::RtcError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "RtcError occurred {:?}: {}", self.error_type, self.message)
    }
}

#[cfg(test)]
mod tests {
    use crate::rtc_error::ffi::{RtcError, RtcErrorDetailType, RtcErrorType};

    #[cxx::bridge(namespace = "livekit_ffi")]
    pub mod ffi {
        unsafe extern "C++" {
            include!("livekit/rtc_error.h");

            fn serialize_deserialize() -> String;
        }
    }

    /// Tests that RtcError can correctly deserialize the hex-encoded
    /// error format produced by C++ (see serialize_error in rtc_error.cpp).
    #[test]
    fn serialize_deserialize() {
        let str = ffi::serialize_deserialize();
        let error = unsafe { RtcError::from(&str) };

        assert_eq!(error.error_type, RtcErrorType::InternalError);
        assert_eq!(error.error_detail, RtcErrorDetailType::DataChannelFailure);
        assert!(error.has_sctp_cause_code);
        assert_eq!(error.sctp_cause_code, 24);
        assert_eq!(error.message, "this is not a test, I repeat, this is not a test");
    }
}
