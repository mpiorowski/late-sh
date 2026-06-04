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

#include <stdint.h>

#include <atomic>
#include <deque>
#include <memory>
#include <optional>
#include <string>
#include <unordered_map>
#include <vector>

#include "absl/types/optional.h"
#include "api/frame_transformer_interface.h"
#include "api/rtp_sender_interface.h"
#include "api/rtp_receiver_interface.h"
#include "api/scoped_refptr.h"
#include "livekit/webrtc.h"
#include "rtc_base/synchronization/mutex.h"
#include "rust/cxx.h"

// Forward declarations to avoid circular includes
// (video_track.h -> packet_trailer.h -> peer_connection.h -> media_stream.h -> video_track.h)
namespace livekit_ffi {
class PeerConnectionFactory;
class RtpSender;
class RtpReceiver;
enum class VideoPublishTimingStage : int32_t;
enum class VideoSubscribeTimingStage : int32_t;
struct VideoPublishTimingObserverWrapper;
struct VideoSubscribeTimingObserverWrapper;
}  // namespace livekit_ffi

namespace livekit_ffi {

// Magic bytes to identify packet trailers: "LKTS" (LiveKit TimeStamp)
constexpr uint8_t kPacketTrailerMagic[4] = {'L', 'K', 'T', 'S'};

// Trailer envelope: [trailer_len: 1B] [magic: 4B] = 5 bytes.
// Always present at the end of every trailer.
constexpr size_t kTrailerEnvelopeSize = 5;

// TLV element overhead: [tag: 1B] [len: 1B] = 2 bytes before value.
// All TLV bytes (tag, len, value) are XORed with 0xFF.

// TLV tag IDs
constexpr uint8_t kTagTimestampUs = 0x01;  // value: 8 bytes big-endian uint64
constexpr uint8_t kTagFrameId = 0x02;      // value: 4 bytes big-endian uint32

constexpr size_t kTimestampTlvSize = 10;  // tag + len + 8-byte value
constexpr size_t kFrameIdTlvSize = 6;     // tag + len + 4-byte value

// Trailer size varies because frame_id is omitted when it is unset (0).
constexpr size_t kPacketTrailerMinSize =
    kTimestampTlvSize + kTrailerEnvelopeSize;
constexpr size_t kPacketTrailerMaxSize =
    kTimestampTlvSize + kFrameIdTlvSize + kTrailerEnvelopeSize;

struct PacketTrailerMetadata {
  uint64_t user_timestamp;
  uint32_t frame_id;
  uint32_t ssrc;  // SSRC that produced this entry (for simulcast tracking)
};

/// Frame transformer that appends/extracts packet trailers.
/// This transformer can be used standalone or in conjunction with e2ee.
///
/// On the send side, user timestamps are stored in an internal map keyed
/// by capture timestamp (microseconds).  When TransformSend fires it
/// looks up the user timestamp via the frame's CaptureTime().
///
/// On the receive side, extracted frame metadata is stored in an
/// internal map keyed by RTP timestamp (uint32_t).  Decoded frames can
/// look up their metadata via lookup_frame_metadata(rtp_ts).
class PacketTrailerTransformer : public webrtc::FrameTransformerInterface {
 public:
  enum class Direction { kSend, kReceive };

  explicit PacketTrailerTransformer(Direction direction);
  ~PacketTrailerTransformer() override = default;

  // FrameTransformerInterface implementation
  void Transform(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame) override;
  void RegisterTransformedFrameCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) override;
  void RegisterTransformedFrameSinkCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
      uint32_t ssrc) override;
  void UnregisterTransformedFrameCallback() override;
  void UnregisterTransformedFrameSinkCallback(uint32_t ssrc) override;

  /// Enable/disable timestamp embedding
  void set_enabled(bool enabled);
  bool enabled() const;

  /// Lookup the frame metadata associated with a given RTP timestamp.
  /// Returns the metadata if found, nullopt otherwise.
  /// The entry is removed from the map after lookup.
  std::optional<PacketTrailerMetadata> lookup_frame_metadata(uint32_t rtp_timestamp);

  /// Store frame metadata for a given capture timestamp (sender side).
  /// Called from VideoTrackSource::on_captured_frame with the
  /// TimestampAligner-adjusted timestamp, which matches CaptureTime()
  /// in the encoder pipeline.
  void store_frame_metadata(int64_t capture_timestamp_us,
                            uint64_t user_timestamp,
                            uint32_t frame_id);

  /// Set the observer receiving sender-side publish timing events.
  void set_publish_timing_observer(
      rust::Box<VideoPublishTimingObserverWrapper> observer);

  /// Clear the observer receiving sender-side publish timing events.
  void clear_publish_timing_observer();

  /// Emit a sender-side publish timing event.
  void emit_publish_timing(VideoPublishTimingStage stage,
                           uint64_t user_timestamp,
                           uint32_t frame_id) const;

  /// Set the observer receiving receiver-side subscribe timing events.
  void set_subscribe_timing_observer(
      rust::Box<VideoSubscribeTimingObserverWrapper> observer);

  /// Clear the observer receiving receiver-side subscribe timing events.
  void clear_subscribe_timing_observer();

  /// Emit a receiver-side subscribe timing event.
  void emit_subscribe_timing(VideoSubscribeTimingStage stage,
                             uint64_t user_timestamp,
                             uint32_t frame_id) const;

 private:
  void TransformSend(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame);
  void TransformReceive(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame);
  void emit_subscribe_timing(VideoSubscribeTimingStage stage,
                             uint64_t user_timestamp,
                             uint32_t frame_id,
                             uint64_t timestamp_us) const;
  bool publish_timing_enabled() const;
  bool subscribe_timing_enabled() const;

  PacketTrailerMetadata LookupSendMetadata(
      const webrtc::TransformableFrameInterface& frame,
      uint32_t ssrc,
      uint32_t rtp_timestamp) const;

  /// Append frame metadata trailer to frame data
  std::vector<uint8_t> AppendTrailer(
      webrtc::ArrayView<const uint8_t> data,
      uint64_t user_timestamp,
      uint32_t frame_id);

  /// Extract and remove frame metadata trailer from frame data
  std::optional<PacketTrailerMetadata> ExtractTrailer(
      webrtc::ArrayView<const uint8_t> data,
      std::vector<uint8_t>& out_data);

  const Direction direction_;
  std::atomic<bool> enabled_{true};
  mutable webrtc::Mutex mutex_;
  webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback_;
  std::unordered_map<uint32_t,
                     webrtc::scoped_refptr<webrtc::TransformedFrameCallback>>
      sink_callbacks_;
  // Send-side map: capture timestamp (us) -> frame metadata.
  // Populated by store_frame_metadata(), consumed by TransformSend()
  // via CaptureTime() lookup.
  mutable webrtc::Mutex send_map_mutex_;
  mutable std::unordered_map<int64_t, PacketTrailerMetadata> send_map_;
  mutable std::deque<int64_t> send_map_order_;
  static constexpr size_t kMaxSendMapEntries = 300;

  // Receive-side map: RTP timestamp -> frame metadata.
  // Keyed by RTP timestamp so decoded frames can look up their
  // metadata regardless of frame drops or reordering.
  mutable webrtc::Mutex recv_map_mutex_;
  mutable std::unordered_map<uint32_t, PacketTrailerMetadata> recv_map_;
  mutable std::deque<uint32_t> recv_map_order_;
  static constexpr size_t kMaxRecvMapEntries = 300;

  // Simulcast tracking: detect layer switches and flush stale entries.
  mutable uint32_t recv_active_ssrc_{0};

  mutable webrtc::Mutex publish_timing_observer_mutex_;
  std::atomic<bool> publish_timing_enabled_{false};
  mutable std::shared_ptr<rust::Box<VideoPublishTimingObserverWrapper>>
      publish_timing_observer_;
  mutable webrtc::Mutex subscribe_timing_observer_mutex_;
  std::atomic<bool> subscribe_timing_enabled_{false};
  mutable std::shared_ptr<rust::Box<VideoSubscribeTimingObserverWrapper>>
      subscribe_timing_observer_;
};

/// Wrapper class for Rust FFI that manages packet trailer transformers.
class PacketTrailerHandler {
 public:
  PacketTrailerHandler(
      std::shared_ptr<RtcRuntime> rtc_runtime,
      webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender);

  PacketTrailerHandler(
      std::shared_ptr<RtcRuntime> rtc_runtime,
      webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver);

  ~PacketTrailerHandler() = default;

  /// Enable/disable timestamp embedding
  void set_enabled(bool enabled) const;
  bool enabled() const;

  /// Lookup the user timestamp for a given RTP timestamp (receiver side).
  /// Returns UINT64_MAX if not found. The entry is removed after lookup.
  /// Also caches the frame_id for retrieval via last_lookup_frame_id().
  uint64_t lookup_timestamp(uint32_t rtp_timestamp) const;

  /// Returns the frame_id from the most recent successful
  /// lookup_timestamp() call. Returns 0 if no lookup succeeded.
  uint32_t last_lookup_frame_id() const;

  /// Store frame metadata for a given capture timestamp (sender side).
  void store_frame_metadata(int64_t capture_timestamp_us,
                            uint64_t user_timestamp,
                            uint32_t frame_id) const;

  /// Set the observer receiving sender-side publish timing events.
  void set_publish_timing_observer(
      rust::Box<VideoPublishTimingObserverWrapper> observer) const;

  /// Clear the observer receiving sender-side publish timing events.
  void clear_publish_timing_observer() const;

  /// Emit a sender-side publish timing event.
  void emit_publish_timing(VideoPublishTimingStage stage,
                           uint64_t user_timestamp,
                           uint32_t frame_id) const;

  /// Set the observer receiving receiver-side subscribe timing events.
  void set_subscribe_timing_observer(
      rust::Box<VideoSubscribeTimingObserverWrapper> observer) const;

  /// Clear the observer receiving receiver-side subscribe timing events.
  void clear_subscribe_timing_observer() const;

  /// Emit a receiver-side subscribe timing event.
  void emit_subscribe_timing(VideoSubscribeTimingStage stage,
                             uint64_t user_timestamp,
                             uint32_t frame_id) const;

  /// Access the underlying transformer for chaining.
  webrtc::scoped_refptr<PacketTrailerTransformer> transformer() const;

 private:
  std::shared_ptr<RtcRuntime> rtc_runtime_;
  webrtc::scoped_refptr<PacketTrailerTransformer> transformer_;
  webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
  webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
  mutable uint32_t last_frame_id_{0};
};

// Factory functions for Rust FFI

std::shared_ptr<PacketTrailerHandler> new_packet_trailer_sender(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<RtpSender> sender);

std::shared_ptr<PacketTrailerHandler> new_packet_trailer_receiver(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<RtpReceiver> receiver);

}  // namespace livekit_ffi
