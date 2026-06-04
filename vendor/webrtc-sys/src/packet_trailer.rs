// Copyright 2026 LiveKit, Inc.
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

/// Callback invoked for native video publish pipeline timing events.
pub type OnVideoPublishTiming = Box<dyn Fn(ffi::VideoPublishTimingEvent) + Send + Sync + 'static>;
/// Callback invoked for native video subscribe pipeline timing events.
pub type OnVideoSubscribeTiming =
    Box<dyn Fn(ffi::VideoSubscribeTimingEvent) + Send + Sync + 'static>;

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    #[repr(i32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum VideoPublishTimingStage {
        EncoderUpload,
        EncoderOutput,
        WebrtcPacketize,
    }

    #[repr(i32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum VideoSubscribeTimingStage {
        WebrtcReceive,
        DecoderUpload,
        DecoderOutput,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct VideoPublishTimingEvent {
        pub stage: VideoPublishTimingStage,
        pub timestamp_us: u64,
        pub capture_timestamp_us: u64,
        pub frame_id: u32,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct VideoSubscribeTimingEvent {
        pub stage: VideoSubscribeTimingStage,
        pub timestamp_us: u64,
        pub capture_timestamp_us: u64,
        pub frame_id: u32,
    }

    unsafe extern "C++" {
        include!("livekit/packet_trailer.h");
        include!("livekit/rtp_sender.h");
        include!("livekit/rtp_receiver.h");
        include!("livekit/peer_connection_factory.h");

        type RtpSender = crate::rtp_sender::ffi::RtpSender;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;
        type PeerConnectionFactory = crate::peer_connection_factory::ffi::PeerConnectionFactory;

        /// Handler for packet trailer embedding/extraction on RTP streams.
        pub type PacketTrailerHandler;

        /// Enable/disable timestamp embedding.
        fn set_enabled(self: &PacketTrailerHandler, enabled: bool);

        /// Check if timestamp embedding is enabled.
        fn enabled(self: &PacketTrailerHandler) -> bool;

        /// Lookup the user timestamp for a given RTP timestamp (receiver side).
        /// Returns -1 if not found. The entry is removed after lookup.
        /// Also caches the frame_id for retrieval via last_lookup_frame_id().
        fn lookup_timestamp(self: &PacketTrailerHandler, rtp_timestamp: u32) -> u64;

        /// Returns the frame_id from the most recent successful
        /// lookup_timestamp() call.
        fn last_lookup_frame_id(self: &PacketTrailerHandler) -> u32;

        /// Store frame metadata for a given capture timestamp (sender side).
        fn store_frame_metadata(
            self: &PacketTrailerHandler,
            capture_timestamp_us: i64,
            user_timestamp: u64,
            frame_id: u32,
        );

        /// Set a callback for sender-side publish timing events.
        fn set_publish_timing_observer(
            self: &PacketTrailerHandler,
            observer: Box<VideoPublishTimingObserverWrapper>,
        );

        /// Clear the sender-side publish timing callback.
        fn clear_publish_timing_observer(self: &PacketTrailerHandler);

        /// Set a callback for receiver-side subscribe timing events.
        fn set_subscribe_timing_observer(
            self: &PacketTrailerHandler,
            observer: Box<VideoSubscribeTimingObserverWrapper>,
        );

        /// Clear the receiver-side subscribe timing callback.
        fn clear_subscribe_timing_observer(self: &PacketTrailerHandler);

        /// Emit a receiver-side subscribe timing event.
        fn emit_subscribe_timing(
            self: &PacketTrailerHandler,
            stage: VideoSubscribeTimingStage,
            user_timestamp: u64,
            frame_id: u32,
        );

        /// Create a new packet trailer handler for a sender.
        fn new_packet_trailer_sender(
            peer_factory: SharedPtr<PeerConnectionFactory>,
            sender: SharedPtr<RtpSender>,
        ) -> SharedPtr<PacketTrailerHandler>;

        /// Create a new packet trailer handler for a receiver.
        fn new_packet_trailer_receiver(
            peer_factory: SharedPtr<PeerConnectionFactory>,
            receiver: SharedPtr<RtpReceiver>,
        ) -> SharedPtr<PacketTrailerHandler>;
    }

    extern "Rust" {
        type VideoPublishTimingObserverWrapper;
        type VideoSubscribeTimingObserverWrapper;

        fn on_publish_timing(
            self: &VideoPublishTimingObserverWrapper,
            event: VideoPublishTimingEvent,
        );

        fn on_subscribe_timing(
            self: &VideoSubscribeTimingObserverWrapper,
            event: VideoSubscribeTimingEvent,
        );
    }
}

impl_thread_safety!(ffi::PacketTrailerHandler, Send + Sync);

pub struct VideoPublishTimingObserverWrapper {
    observer: OnVideoPublishTiming,
}

impl VideoPublishTimingObserverWrapper {
    pub fn new(observer: OnVideoPublishTiming) -> Self {
        Self { observer }
    }

    fn on_publish_timing(&self, event: ffi::VideoPublishTimingEvent) {
        (self.observer)(event);
    }
}

pub struct VideoSubscribeTimingObserverWrapper {
    observer: OnVideoSubscribeTiming,
}

impl VideoSubscribeTimingObserverWrapper {
    pub fn new(observer: OnVideoSubscribeTiming) -> Self {
        Self { observer }
    }

    fn on_subscribe_timing(&self, event: ffi::VideoSubscribeTimingEvent) {
        (self.observer)(event);
    }
}
