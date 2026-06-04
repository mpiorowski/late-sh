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

#include "livekit/packet_trailer.h"

#include <chrono>
#include <cstring>
#include <optional>

#include "api/make_ref_counted.h"
#include "livekit/peer_connection_factory.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "rtc_base/logging.h"
#include "webrtc-sys/src/packet_trailer.rs.h"

namespace livekit_ffi {

namespace {

uint64_t CurrentUnixTimeMicros() {
  auto now = std::chrono::system_clock::now().time_since_epoch();
  return static_cast<uint64_t>(
      std::chrono::duration_cast<std::chrono::microseconds>(now).count());
}

}  // namespace

// PacketTrailerTransformer implementation

PacketTrailerTransformer::PacketTrailerTransformer(Direction direction)
    : direction_(direction) {}

void PacketTrailerTransformer::Transform(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  uint32_t ssrc = frame->GetSsrc();
  uint32_t rtp_timestamp = frame->GetTimestamp();

  if (direction_ == Direction::kSend) {
    TransformSend(std::move(frame));
    return;
  }

  if (!enabled_.load()) {
    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> cb;
    {
      webrtc::MutexLock lock(&mutex_);
      auto it = sink_callbacks_.find(ssrc);
      if (it != sink_callbacks_.end()) {
        cb = it->second;
      } else {
        cb = callback_;
      }
    }

    if (cb) {
      cb->OnTransformedFrame(std::move(frame));
    } else {
      RTC_LOG(LS_WARNING)
          << "PacketTrailerTransformer::Transform (disabled) has no callback"
          << " direction="
          << (direction_ == Direction::kSend ? "send" : "recv")
          << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;
    }
    return;
  }

  TransformReceive(std::move(frame));
}

void PacketTrailerTransformer::TransformSend(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  uint32_t rtp_timestamp = frame->GetTimestamp();
  uint32_t ssrc = frame->GetSsrc();

  auto data = frame->GetData();
  PacketTrailerMetadata meta_to_embed =
      LookupSendMetadata(*frame, ssrc, rtp_timestamp);
  emit_publish_timing(VideoPublishTimingStage::EncoderOutput,
                      meta_to_embed.user_timestamp, meta_to_embed.frame_id);

  // Always append trailer when enabled (even if timestamp is 0,
  // which indicates no metadata was set for this frame)
  std::vector<uint8_t> new_data;
  if (enabled_.load()) {
    new_data = AppendTrailer(data, meta_to_embed.user_timestamp,
                             meta_to_embed.frame_id);
    frame->SetData(webrtc::ArrayView<const uint8_t>(new_data));
  }

  // Forward to the appropriate callback (either global or per-SSRC sink).
  webrtc::scoped_refptr<webrtc::TransformedFrameCallback> cb;
  {
    webrtc::MutexLock lock(&mutex_);
    auto it = sink_callbacks_.find(ssrc);
    if (it != sink_callbacks_.end()) {
      cb = it->second;
    } else {
      cb = callback_;
    }
  }

  if (cb) {
    emit_publish_timing(VideoPublishTimingStage::WebrtcPacketize,
                        meta_to_embed.user_timestamp, meta_to_embed.frame_id);
    cb->OnTransformedFrame(std::move(frame));
  } else {
    RTC_LOG(LS_WARNING)
        << "PacketTrailerTransformer::TransformSend has no callback"
        << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;
  }
}

PacketTrailerMetadata PacketTrailerTransformer::LookupSendMetadata(
    const webrtc::TransformableFrameInterface& frame,
    uint32_t ssrc,
    uint32_t rtp_timestamp) const {
  // Look up the frame metadata by the frame's capture time.
  // CaptureTime() returns Timestamp::Millis(capture_time_ms_) where
  // capture_time_ms_ = timestamp_us / 1000.  So capture_time->us()
  // has millisecond precision (bottom 3 digits always zero).
  // store_frame_metadata() truncates its key the same way.
  PacketTrailerMetadata meta_to_embed{0, 0, 0};
  auto capture_time = frame.CaptureTime();
  if (capture_time.has_value()) {
    int64_t capture_us = capture_time->us();

    webrtc::MutexLock lock(&send_map_mutex_);
    auto it = send_map_.find(capture_us);
    if (it != send_map_.end()) {
      meta_to_embed = it->second;
      // Don't erase — simulcast layers share the same capture time.
      // Entries are pruned by capacity in store_frame_metadata().
    }
  } else {
    RTC_LOG(LS_WARNING)
        << "PacketTrailerTransformer::TransformSend CaptureTime() not available"
        << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;
  }
  return meta_to_embed;
}

void PacketTrailerTransformer::TransformReceive(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  uint32_t ssrc = frame->GetSsrc();
  uint32_t rtp_timestamp = frame->GetTimestamp();
  auto data = frame->GetData();
  std::vector<uint8_t> stripped_data;

  auto meta = ExtractTrailer(data, stripped_data);
  PacketTrailerMetadata timing_meta{0, 0, ssrc};

  if (meta.has_value()) {
    meta->ssrc = ssrc;
    timing_meta = meta.value();

    {
      webrtc::MutexLock lock(&recv_map_mutex_);

      // Detect simulcast layer switch (SSRC change).
      // When the SFU switches us to a different layer, the old layer's
      // entries are stale and can cause RTP timestamp collisions or
      // return wrong user timestamps on lookup.  Flush them.
      if (recv_active_ssrc_ != 0 && recv_active_ssrc_ != ssrc) {
        auto oit = recv_map_order_.begin();
        while (oit != recv_map_order_.end()) {
          auto mit = recv_map_.find(*oit);
          if (mit != recv_map_.end() && mit->second.ssrc != ssrc) {
            recv_map_.erase(mit);
            oit = recv_map_order_.erase(oit);
          } else {
            ++oit;
          }
        }
      }
      recv_active_ssrc_ = ssrc;

      bool collision = recv_map_.find(rtp_timestamp) != recv_map_.end();

      // Evict oldest entry if at capacity
      while (recv_map_.size() >= kMaxRecvMapEntries &&
             !recv_map_order_.empty()) {
        auto evicted_rtp = recv_map_order_.front();
        recv_map_.erase(evicted_rtp);
        recv_map_order_.pop_front();
      }
      if (!collision) {
        recv_map_order_.push_back(rtp_timestamp);
      }
      recv_map_[rtp_timestamp] = meta.value();
    }

    // Update frame with stripped data
    frame->SetData(webrtc::ArrayView<const uint8_t>(stripped_data));
  }
  uint64_t receive_timestamp_us =
      subscribe_timing_enabled() ? CurrentUnixTimeMicros() : 0;
  emit_subscribe_timing(VideoSubscribeTimingStage::WebrtcReceive,
                        timing_meta.user_timestamp, timing_meta.frame_id,
                        receive_timestamp_us);

  // Forward to the appropriate callback (either global or per-SSRC sink).
  webrtc::scoped_refptr<webrtc::TransformedFrameCallback> cb;
  {
    webrtc::MutexLock lock(&mutex_);
    auto it = sink_callbacks_.find(ssrc);
    if (it != sink_callbacks_.end()) {
      cb = it->second;
    } else {
      cb = callback_;
    }
  }

  if (cb) {
    emit_subscribe_timing(VideoSubscribeTimingStage::DecoderUpload,
                          timing_meta.user_timestamp, timing_meta.frame_id);
    cb->OnTransformedFrame(std::move(frame));
  } else {
    RTC_LOG(LS_WARNING)
        << "PacketTrailerTransformer::TransformReceive has no callback"
        << " ssrc=" << ssrc << " rtp_ts=" << rtp_timestamp;
  }
}

std::vector<uint8_t> PacketTrailerTransformer::AppendTrailer(
    webrtc::ArrayView<const uint8_t> data,
    uint64_t user_timestamp,
    uint32_t frame_id) {
  const bool has_frame_id = frame_id != 0;
  const size_t trailer_len = kTimestampTlvSize +
                             (has_frame_id ? kFrameIdTlvSize : 0) +
                             kTrailerEnvelopeSize;
  std::vector<uint8_t> result;
  result.reserve(data.size() + trailer_len);

  // Copy original data
  result.insert(result.end(), data.begin(), data.end());

  // All TLV bytes are XORed with 0xFF to prevent H.264 NAL start code
  // sequences (0x000001 / 0x00000001) from appearing inside the trailer.

  // TLV: timestamp_us (tag=0x01, len=8, 8 bytes big-endian)
  result.push_back(kTagTimestampUs ^ 0xFF);
  result.push_back(8 ^ 0xFF);
  for (int i = 7; i >= 0; --i) {
    result.push_back(
        static_cast<uint8_t>(((user_timestamp >> (i * 8)) & 0xFF) ^ 0xFF));
  }

  if (has_frame_id) {
    // TLV: frame_id (tag=0x02, len=4, 4 bytes big-endian)
    result.push_back(kTagFrameId ^ 0xFF);
    result.push_back(4 ^ 0xFF);
    for (int i = 3; i >= 0; --i) {
      result.push_back(
          static_cast<uint8_t>(((frame_id >> (i * 8)) & 0xFF) ^ 0xFF));
    }
  }

  // Envelope: trailer_len (1B, XORed) + magic (4B, NOT XORed)
  result.push_back(static_cast<uint8_t>(trailer_len ^ 0xFF));
  result.insert(result.end(), std::begin(kPacketTrailerMagic),
                std::end(kPacketTrailerMagic));

  return result;
}

std::optional<PacketTrailerMetadata> PacketTrailerTransformer::ExtractTrailer(
    webrtc::ArrayView<const uint8_t> data,
    std::vector<uint8_t>& out_data) {
  if (data.size() < kTrailerEnvelopeSize) {
    out_data.assign(data.begin(), data.end());
    return std::nullopt;
  }

  // Check for magic bytes at the end
  const uint8_t* magic_start = data.data() + data.size() - 4;
  if (std::memcmp(magic_start, kPacketTrailerMagic, 4) != 0) {
    out_data.assign(data.begin(), data.end());
    return std::nullopt;
  }

  uint8_t trailer_len = data[data.size() - 5] ^ 0xFF;

  if (trailer_len < kTrailerEnvelopeSize || trailer_len > data.size()) {
    out_data.assign(data.begin(), data.end());
    return std::nullopt;
  }

  // Walk the TLV region: everything from trailer_start up to the envelope.
  const uint8_t* trailer_start = data.data() + data.size() - trailer_len;
  size_t tlv_region_len = trailer_len - kTrailerEnvelopeSize;

  PacketTrailerMetadata meta{0, 0, 0};
  bool found_any = false;
  size_t pos = 0;

  while (pos + 2 <= tlv_region_len) {
    uint8_t tag = trailer_start[pos] ^ 0xFF;
    uint8_t len = trailer_start[pos + 1] ^ 0xFF;
    pos += 2;

    if (pos + len > tlv_region_len) {
      break;
    }

    const uint8_t* val = trailer_start + pos;

    if (tag == kTagTimestampUs && len == 8) {
      uint64_t ts = 0;
      for (int i = 0; i < 8; ++i) {
        ts = (ts << 8) | (val[i] ^ 0xFF);
      }
      meta.user_timestamp = ts;
      found_any = true;
    } else if (tag == kTagFrameId && len == 4) {
      uint32_t fid = 0;
      for (int i = 0; i < 4; ++i) {
        fid = (fid << 8) | (val[i] ^ 0xFF);
      }
      meta.frame_id = fid;
      found_any = true;
    }
    // Unknown tags are silently skipped.

    pos += len;
  }

  out_data.assign(data.begin(), data.end() - trailer_len);

  if (!found_any) {
    return std::nullopt;
  }
  return meta;
}

void PacketTrailerTransformer::RegisterTransformedFrameCallback(
    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) {
  webrtc::MutexLock lock(&mutex_);
  callback_ = callback;
}

void PacketTrailerTransformer::RegisterTransformedFrameSinkCallback(
    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
    uint32_t ssrc) {
  webrtc::MutexLock lock(&mutex_);
  sink_callbacks_[ssrc] = callback;
}

void PacketTrailerTransformer::UnregisterTransformedFrameCallback() {
  webrtc::MutexLock lock(&mutex_);
  callback_ = nullptr;
}

void PacketTrailerTransformer::UnregisterTransformedFrameSinkCallback(
    uint32_t ssrc) {
  webrtc::MutexLock lock(&mutex_);
  sink_callbacks_.erase(ssrc);
}

void PacketTrailerTransformer::set_enabled(bool enabled) {
  enabled_.store(enabled);
}

bool PacketTrailerTransformer::enabled() const {
  return enabled_.load();
}

std::optional<PacketTrailerMetadata> PacketTrailerTransformer::lookup_frame_metadata(
    uint32_t rtp_timestamp) {
  webrtc::MutexLock lock(&recv_map_mutex_);
  auto it = recv_map_.find(rtp_timestamp);
  if (it == recv_map_.end()) {
    return std::nullopt;
  }
  PacketTrailerMetadata meta = it->second;
  recv_map_.erase(it);
  for (auto oit = recv_map_order_.begin(); oit != recv_map_order_.end();
       ++oit) {
    if (*oit == rtp_timestamp) {
      recv_map_order_.erase(oit);
      break;
    }
  }
  return meta;
}

void PacketTrailerTransformer::store_frame_metadata(
    int64_t capture_timestamp_us,
    uint64_t user_timestamp,
    uint32_t frame_id) {
  // Truncate to millisecond precision to match what WebRTC stores
  // internally.  The encoder pipeline converts the VideoFrame's
  // timestamp_us to capture_time_ms_ = timestamp_us / 1000, and
  // CaptureTime() returns Timestamp::Millis(capture_time_ms_).
  // When we call capture_time->us() in TransformSend we get a value
  // with the bottom 3 digits zeroed, so we must store with the same
  // truncation to ensure the lookup succeeds.
  //
  // The caller (VideoTrackSource::on_captured_frame) passes the
  // TimestampAligner-adjusted timestamp here, which is the same
  // value that becomes CaptureTime() in the encoder pipeline.
  int64_t key = (capture_timestamp_us / 1000) * 1000;

  webrtc::MutexLock lock(&send_map_mutex_);

  // Evict oldest entries if at capacity
  while (send_map_.size() >= kMaxSendMapEntries && !send_map_order_.empty()) {
    send_map_.erase(send_map_order_.front());
    send_map_order_.pop_front();
  }

  if (send_map_.find(key) == send_map_.end()) {
    send_map_order_.push_back(key);
  }
  send_map_[key] = PacketTrailerMetadata{user_timestamp, frame_id, 0};
}

void PacketTrailerTransformer::set_publish_timing_observer(
    rust::Box<VideoPublishTimingObserverWrapper> observer) {
  webrtc::MutexLock lock(&publish_timing_observer_mutex_);
  publish_timing_observer_ =
      std::make_shared<rust::Box<VideoPublishTimingObserverWrapper>>(
          std::move(observer));
  publish_timing_enabled_.store(true);
}

void PacketTrailerTransformer::clear_publish_timing_observer() {
  webrtc::MutexLock lock(&publish_timing_observer_mutex_);
  publish_timing_observer_.reset();
  publish_timing_enabled_.store(false);
}

void PacketTrailerTransformer::emit_publish_timing(
    VideoPublishTimingStage stage,
    uint64_t user_timestamp,
    uint32_t frame_id) const {
  if (!publish_timing_enabled()) {
    return;
  }

  std::shared_ptr<rust::Box<VideoPublishTimingObserverWrapper>> observer;
  {
    webrtc::MutexLock lock(&publish_timing_observer_mutex_);
    observer = publish_timing_observer_;
  }
  if (!observer) {
    return;
  }

  (*observer)->on_publish_timing(VideoPublishTimingEvent{
      stage, CurrentUnixTimeMicros(), user_timestamp, frame_id});
}

void PacketTrailerTransformer::set_subscribe_timing_observer(
    rust::Box<VideoSubscribeTimingObserverWrapper> observer) {
  webrtc::MutexLock lock(&subscribe_timing_observer_mutex_);
  subscribe_timing_observer_ =
      std::make_shared<rust::Box<VideoSubscribeTimingObserverWrapper>>(
          std::move(observer));
  subscribe_timing_enabled_.store(true);
}

void PacketTrailerTransformer::clear_subscribe_timing_observer() {
  webrtc::MutexLock lock(&subscribe_timing_observer_mutex_);
  subscribe_timing_observer_.reset();
  subscribe_timing_enabled_.store(false);
}

void PacketTrailerTransformer::emit_subscribe_timing(
    VideoSubscribeTimingStage stage,
    uint64_t user_timestamp,
    uint32_t frame_id) const {
  if (!subscribe_timing_enabled()) {
    return;
  }

  emit_subscribe_timing(stage, user_timestamp, frame_id,
                        CurrentUnixTimeMicros());
}

void PacketTrailerTransformer::emit_subscribe_timing(
    VideoSubscribeTimingStage stage,
    uint64_t user_timestamp,
    uint32_t frame_id,
    uint64_t timestamp_us) const {
  if (!subscribe_timing_enabled()) {
    return;
  }

  std::shared_ptr<rust::Box<VideoSubscribeTimingObserverWrapper>> observer;
  {
    webrtc::MutexLock lock(&subscribe_timing_observer_mutex_);
    observer = subscribe_timing_observer_;
  }
  if (!observer) {
    return;
  }

  (*observer)->on_subscribe_timing(VideoSubscribeTimingEvent{
      stage, timestamp_us, user_timestamp, frame_id});
}

bool PacketTrailerTransformer::publish_timing_enabled() const {
  return publish_timing_enabled_.load();
}

bool PacketTrailerTransformer::subscribe_timing_enabled() const {
  return subscribe_timing_enabled_.load();
}

// PacketTrailerHandler implementation

PacketTrailerHandler::PacketTrailerHandler(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender)
    : rtc_runtime_(rtc_runtime), sender_(sender) {
  transformer_ = webrtc::make_ref_counted<PacketTrailerTransformer>(
      PacketTrailerTransformer::Direction::kSend);
  sender->SetEncoderToPacketizerFrameTransformer(transformer_);
}

PacketTrailerHandler::PacketTrailerHandler(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver)
    : rtc_runtime_(rtc_runtime), receiver_(receiver) {
  transformer_ = webrtc::make_ref_counted<PacketTrailerTransformer>(
      PacketTrailerTransformer::Direction::kReceive);
  receiver->SetDepacketizerToDecoderFrameTransformer(transformer_);
}

void PacketTrailerHandler::set_enabled(bool enabled) const {
  transformer_->set_enabled(enabled);
}

bool PacketTrailerHandler::enabled() const {
  return transformer_->enabled();
}

uint64_t PacketTrailerHandler::lookup_timestamp(uint32_t rtp_timestamp) const {
  auto meta = transformer_->lookup_frame_metadata(rtp_timestamp);
  if (meta.has_value()) {
    last_frame_id_ = meta->frame_id;
    return meta->user_timestamp;
  }
  return UINT64_MAX;
}

uint32_t PacketTrailerHandler::last_lookup_frame_id() const {
  return last_frame_id_;
}

void PacketTrailerHandler::store_frame_metadata(
    int64_t capture_timestamp_us,
    uint64_t user_timestamp,
    uint32_t frame_id) const {
  transformer_->store_frame_metadata(capture_timestamp_us, user_timestamp, frame_id);
}

void PacketTrailerHandler::set_publish_timing_observer(
    rust::Box<VideoPublishTimingObserverWrapper> observer) const {
  transformer_->set_publish_timing_observer(std::move(observer));
}

void PacketTrailerHandler::clear_publish_timing_observer() const {
  transformer_->clear_publish_timing_observer();
}

void PacketTrailerHandler::emit_publish_timing(
    VideoPublishTimingStage stage,
    uint64_t user_timestamp,
    uint32_t frame_id) const {
  transformer_->emit_publish_timing(stage, user_timestamp, frame_id);
}

void PacketTrailerHandler::set_subscribe_timing_observer(
    rust::Box<VideoSubscribeTimingObserverWrapper> observer) const {
  transformer_->set_subscribe_timing_observer(std::move(observer));
}

void PacketTrailerHandler::clear_subscribe_timing_observer() const {
  transformer_->clear_subscribe_timing_observer();
}

void PacketTrailerHandler::emit_subscribe_timing(
    VideoSubscribeTimingStage stage,
    uint64_t user_timestamp,
    uint32_t frame_id) const {
  transformer_->emit_subscribe_timing(stage, user_timestamp, frame_id);
}

webrtc::scoped_refptr<PacketTrailerTransformer> PacketTrailerHandler::transformer() const {
  return transformer_;
}

// Factory functions

std::shared_ptr<PacketTrailerHandler> new_packet_trailer_sender(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<RtpSender> sender) {
  return std::make_shared<PacketTrailerHandler>(
      peer_factory->rtc_runtime(), sender->rtc_sender());
}

std::shared_ptr<PacketTrailerHandler> new_packet_trailer_receiver(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<RtpReceiver> receiver) {
  return std::make_shared<PacketTrailerHandler>(
      peer_factory->rtc_runtime(), receiver->rtc_receiver());
}

}  // namespace livekit_ffi
