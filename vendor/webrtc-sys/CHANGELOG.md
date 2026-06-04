# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.23](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.22...rust-sdks/webrtc-sys@0.3.23) - 2026-02-16

### Other

- add is_screencast to VideoSource ([#896](https://github.com/livekit/rust-sdks/pull/896))

## [0.3.22](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.21...rust-sdks/webrtc-sys@0.3.22) - 2026-02-09

### Fixed

- fix the 440->441 samples issue and pass a noop callback for release ([#848](https://github.com/livekit/rust-sdks/pull/848))

### Other

- Use workspace dependencies & settings ([#856](https://github.com/livekit/rust-sdks/pull/856))

## [0.3.21](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.20...rust-sdks/webrtc-sys@0.3.21) - 2026-01-15

### Other

- Rename namespace for ffi bridge code. ([#836](https://github.com/livekit/rust-sdks/pull/836))

## [0.3.20](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.19...rust-sdks/webrtc-sys@0.3.20) - 2025-12-19

### Fixed

- Exclude the desktop-capturer module link for mobile. ([#817](https://github.com/livekit/rust-sdks/pull/817))

## [0.3.19](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.18...rust-sdks/webrtc-sys@0.3.19) - 2025-12-17

### Fixed

- lazy loading for additional dependencies. ([#814](https://github.com/livekit/rust-sdks/pull/814))

### Other

- Expose WebRTC's audio_mixer ([#806](https://github.com/livekit/rust-sdks/pull/806))

## [0.3.18](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.17...rust-sdks/webrtc-sys@0.3.18) - 2025-12-04

### Other

- initialize frame_.samples_per_channel_ ([#779](https://github.com/livekit/rust-sdks/pull/779))
- Expose desktop capturer ([#725](https://github.com/livekit/rust-sdks/pull/725))

## [0.3.17](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.16...rust-sdks/webrtc-sys@0.3.17) - 2025-11-20

### Fixed

- fix Android libwebrtc builds ([#791](https://github.com/livekit/rust-sdks/pull/791))

### Other

- Enable H265 encoding & decoding on Nvidia GPU ([#776](https://github.com/livekit/rust-sdks/pull/776))
- copy desktop_capture.ninja into C++ library archive ([#787](https://github.com/livekit/rust-sdks/pull/787))
- enable Pipewire (Wayland) and X11 desktop capture ([#784](https://github.com/livekit/rust-sdks/pull/784))

## [0.3.16](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.15...rust-sdks/webrtc-sys@0.3.16) - 2025-10-27

### Fixed

- fix unable to locate __arm_tpidr2_save for android ffi. ([#765](https://github.com/livekit/rust-sdks/pull/765))

### Other

- Linux hardware acceleration build fixes ([#753](https://github.com/livekit/rust-sdks/pull/753))

## [0.3.15](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.14...rust-sdks/webrtc-sys@0.3.15) - 2025-10-22

### Other

- License check ([#746](https://github.com/livekit/rust-sdks/pull/746))
- put examples in root Cargo workspace ([#731](https://github.com/livekit/rust-sdks/pull/731))

## [0.3.14](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.13...rust-sdks/webrtc-sys@0.3.14) - 2025-10-13

### Added

- *(e2ee)* add data channel encryption ([#708](https://github.com/livekit/rust-sdks/pull/708))

### Fixed

- fix some potential audio issues, clean up the code a bit, and suppress some warnings  ([#737](https://github.com/livekit/rust-sdks/pull/737))
- fix linux so link issue. ([#733](https://github.com/livekit/rust-sdks/pull/733))
- change search_dirs to use cc --print-search-dirs instead of clang --print-search-dirs ([#697](https://github.com/livekit/rust-sdks/pull/697))

### Other

- bump libwebrtc libs version for webrtc-sys. ([#741](https://github.com/livekit/rust-sdks/pull/741))
- Enable buffer scaling ([#473](https://github.com/livekit/rust-sdks/pull/473))

## [0.3.13](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.12...rust-sdks/webrtc-sys@0.3.13) - 2025-10-03

### Other

- Fix empty audio frames after resample ([#722](https://github.com/livekit/rust-sdks/pull/722))

## [0.3.12](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.11...rust-sdks/webrtc-sys@0.3.12) - 2025-09-29

### Fixed

- fix Builds/E2E Tests CI. ([#715](https://github.com/livekit/rust-sdks/pull/715))

### Other

- nvidia codec improve ([#721](https://github.com/livekit/rust-sdks/pull/721))
- Upgrade libwebrtc to m137. ([#696](https://github.com/livekit/rust-sdks/pull/696))

## [0.3.11](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.10...rust-sdks/webrtc-sys@0.3.11) - 2025-09-09

### Other

- Optional flags for video hw codec. ([#701](https://github.com/livekit/rust-sdks/pull/701))

## [0.3.10](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.9...rust-sdks/webrtc-sys@0.3.10) - 2025-09-03

### Added

- VA-API support for linux. ([#638](https://github.com/livekit/rust-sdks/pull/638))

### Fixed

- hardware rendering ([#695](https://github.com/livekit/rust-sdks/pull/695))
# Changelog

## [0.3.9](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.8...rust-sdks/webrtc-sys@0.3.9) - 2025-06-17

### Other

- updated the following local packages: webrtc-sys-build

## [0.3.8](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.7...rust-sdks/webrtc-sys@0.3.8) - 2025-06-11

### Fixed

- fix libwebrtc.jar build issue ([#586](https://github.com/livekit/rust-sdks/pull/586))

### Other

- bump version for webrtc (fix win CI) ([#650](https://github.com/livekit/rust-sdks/pull/650))
- try to fix webrtc build for iOS/macOS. ([#646](https://github.com/livekit/rust-sdks/pull/646))
- remove ([#633](https://github.com/livekit/rust-sdks/pull/633))
- expose apm stream_delay ([#616](https://github.com/livekit/rust-sdks/pull/616))
- Add i420_to_nv12 ([#605](https://github.com/livekit/rust-sdks/pull/605))
- ffi-v0.13.0 ([#590](https://github.com/livekit/rust-sdks/pull/590))
- add AudioProcessingModule ([#580](https://github.com/livekit/rust-sdks/pull/580))

## [0.3.7] - 2025-02-05

### Added

- Expose DataChannel.bufferedAmount property

## [0.3.6] - 2024-12-14

### Added

- bump libwebrtc to m125
## 0.3.33 (2026-05-29)

### Fixes

- Add native video pipeline timing instrumentation for local video measurements, exposing local publish and subscribe timing through async streams and subscriber overlay GPU upload and receive-to-GPU latency metrics through explicit timing observers.

## 0.3.32 (2026-05-21)

### Fixes

#### feat: add Android application context initialization for PlatformAudio support.

Android requires `ContextUtils.initialize(applicationContext)` before WebRTC audio components can be created. This change:

- Adds `livekit_ffi_initialize_android_context()` C FFI function for Unity and other FFI consumers
- Uses `CreateAndroidAudioDeviceModule()` instead of generic `CreateAudioDeviceModule()` on Android
- Handles empty device GUIDs on Android (falls back to index 0)
- Documents Android-specific limitations: single default device, no app-level device selection

Platform notes:
- Android device enumeration returns only one "default" device with empty name/GUID
- Audio routing (speaker/earpiece/Bluetooth) is controlled by Android's AudioManager, not WebRTC

## 0.3.31 (2026-05-14)

### Fixes

- chore: bump libwebrtc version to webrtc-51ef663
- Add `LIVEKIT_PREFERRED_HW_ENCODER` to prefer `nvenc` or `vaapi` hardware video encoding when both are available.

#### fix: fix LICENSE.md generation in webrtc build scripts

- Add fix_license_json_parsing.patch to handle GN warnings in JSON output
- Enable add_licenses.patch for iOS and Android builds (was commented out)
- Restore LICENSE.md copy in iOS build script (regression from #1053)

The license generation script was failing because `gn desc --format=json`
outputs warnings before the JSON when certain build args trigger deprecation
notices. The new patch strips non-JSON content before parsing.

#### Get WebRTC ADM into Rust - #1037 (@xianshijing-lk)

This PR introduces platform audio device management via WebRTC's Audio Device Module (ADM).

#### Features
- **ADM Proxy**: New `AdmProxy` class that switches between Dummy ADM (synthetic mode) and Platform ADM (real audio I/O)
- **PlatformAudio API**: High-level Rust API for microphone capture and speaker playout with AEC/AGC/NS
- **Device enumeration**: List and select recording/playout devices by index or GUID
- **Mode switching**: Seamlessly switch between synthetic mode (FFI callbacks) and platform mode (native speakers) while audio is active
- **FFI platform audio support**: Expose platform audio device enumeration and selection through `livekit-ffi`
- **Audio processing**: Configure echo cancellation, noise suppression, and auto gain control with platform-specific defaults (hardware on iOS, software elsewhere)

#### Audio Modes
| Mode | Recording | Playout | Use Case |
|------|-----------|---------|----------|
| Synthetic | NativeAudioSource | Dummy ADM + FFI | Unity audio, agents |
| Platform | Platform ADM mic | Platform ADM speakers | VoIP with AEC |

#### API
```rust
// Create PlatformAudio for microphone/speaker access
let audio = PlatformAudio::new()?;

// Enumerate and select devices
for i in 0..audio.recording_devices() as u16 {
    println!("Mic {}: {}", i, audio.recording_device_name(i));
}
audio.set_recording_device(0)?;

// Create audio track for publishing
let track = LocalAudioTrack::create_audio_track("mic", audio.rtc_source());
```

## 0.3.30 (2026-05-11)

### Fixes

#### Fix missing RTC_OBJC_TYPE macros in webrtc-sys .mm files

Wrap bare ObjC class references in `RTC_OBJC_TYPE()` in `objc_video_factory.mm` and `objc_video_frame_buffer.mm` to support builds with `rtc_objc_prefix` set.

#### Fix WebRTC build scripts to properly report failures and fix C++ module compilation issues

- Add `set -e` to all build scripts so CI properly reports build failures instead of silently creating empty/broken artifacts
- Re-add `use_clang_modules=false` to macOS, iOS, and Linux build scripts to fix C++ module compilation errors

Without `use_clang_modules=false`, builds fail due to libc++ header incompatibilities (on macOS/iOS with Xcode 26.0) or other C++ module issues, resulting in:
- macOS/iOS: Empty `libwebrtc.a` (~13KB instead of ~700MB)
- Android: Missing `libwebrtc.jar`
- Linux: Incomplete artifacts

The builds appeared successful because the scripts continued after ninja failures, but now with `set -e`, failures will be properly reported.

## 0.3.29 (2026-05-10)

### Fixes

- Fix missing `libwebrtc.jar` for Android builds, harden build scripts
- fix race in download_webrtc to reduce flaky build - #1047 (@hechen-eng)
- Improve WebRTC build scripts and add external_audio_source patch - #1053 (@xianshijing-lk)

## 0.3.28 (2026-04-23)

### Features

#### Add support for frame level packet trailer

##890 by @chenosaurus

- Add support to attach/parse frame level timestamps & frame ID to VideoTracks as a custom payload trailer.
- Breaking change in VideoFrame API, must include `frame_metadata` or use VideoFrame::new().

### Fixes

- Add NVENC check when Nvidia GPU detected - #1015 (@chenosaurus)

## 0.3.27 (2026-04-02)

### Features

#### chore: upgrade libwebrtc to m144.

##965 by @cloudwebrtc

### Fixes

#### use the bounded buffer for video stream

##956 by @xianshijing-lk

Before this PR, it uses an unbounded buffer for video stream, that will cause multiple problems:
1, video will be lagged behind if rendering is slow or just wake up from background
2, it will be out of sync with audio

This PRs provides options to set a bounded buffer for video stream, and use 1 buffer as the default option.

## 0.3.26 (2026-03-31)

### Fixes

- fix unity android build with "livekit" prefixed jni - #983 (@xianshijing-lk)

#### fix: fix unavailable sem symbol for Linux aarch64.

##975 by @cloudwebrtc

## 0.3.25 (2026-03-22)

### Fixes

- fix: enable AGC2 adaptive digital controller
- Fix H.264 codec matching

#### E2EE: allow setting key_ring_size and key_derivation_algorithm, update webrtc to m144

##921 by @onestacked

This PR uses [this webrtc-sdk PR](https://github.com/webrtc-sdk/webrtc/pull/224) to configure the KDF.

I've tested this with https://codeberg.org/esoteric_programmer/matrix-jukebox and it is compatible with Element Call.

Fixed: https://github.com/livekit/rust-sdks/issues/796

#### fix clang build issue from zed patches (#949)

##950 by @cloudwebrtc

* webrtc-sys: Use clang instead of gcc

* Debug CI output for aarch64-linux

* ci: Install lld for aarch64-linux FFI builders

* webrtc-sys: Disable CREL

## 0.3.24 (2026-03-13)

### Fixes

#### avoid getting webrtc into underrun

Before this change, the Rust implementation would only start sending silence frames after missing 10 consecutive audio frames. This could cause WebRTC's audio pipeline to enter an underrun state when audio stopped temporarily.

Once WebRTC enters underrun, resuming audio can significantly increase latency until the pipeline stabilizes again. In testing, this could add hundreds of milliseconds of additional latency when audio resumed shortly after the underrun.

This change ensures silence frames are sent earlier to prevent the audio pipeline from entering underrun. By maintaining a continuous stream of audio (including silence), WebRTC can avoid unnecessary buffering and latency spikes when audio resumes.

In testing, this reduces the additional latency observed after underrun recovery and results in more stable end-to-end audio latency.

#### webrtc-sys: Handle gracefully lack of libva on linux

##924 by @kubkon
