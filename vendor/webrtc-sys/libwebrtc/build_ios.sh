#!/bin/bash
# Exit immediately if any command fails. This ensures CI properly reports build
# failures instead of continuing to create empty/broken artifacts.
set -e

# Copyright 2023 LiveKit, Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.


arch=""
profile="release"
environment="device"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --arch)
      arch="$2"
      if [ "$arch" != "arm64" ]; then
        echo "Error: Invalid value for --arch. Must 'arm64'."
        exit 1
      fi
      shift 2
      ;;
    --environment)
      environment="$2"
      if [ "$environment" != "device" ] && [ "$environment" != "simulator" ]; then
        echo "Error: Invalid value for --environment. Must be 'device' or 'simulator'."
        exit 1
      fi
      shift 2
      ;;
    --profile)
      profile="$2"
      if [ "$profile" != "debug" ] && [ "$profile" != "release" ]; then
        echo "Error: Invalid value for --profile. Must be 'debug' or 'release'."
        exit 1
      fi
      shift 2
      ;;
    *)
      echo "Error: Unknown argument '$1'"
      exit 1
      ;;
  esac
done

if [ -z "$arch" ]; then
  echo "Error: --arch must be set."
  exit 1
fi

echo "Building LiveKit WebRTC - iOS"
echo "Arch: $arch"
echo "Profile: $profile"
echo "Environment: $environment"

if [ ! -e "$(pwd)/depot_tools" ]
then
  git clone --depth 1 https://chromium.googlesource.com/chromium/tools/depot_tools.git
fi

export COMMAND_DIR=$(cd $(dirname $0); pwd)
export PATH="$(pwd)/depot_tools:$PATH"

export OUTPUT_DIR="$(pwd)/src/out-$arch-$profile"
export ARTIFACTS_DIR="$(pwd)/ios-$environment-$arch-$profile"

if [ ! -e "$(pwd)/src" ]
then
  gclient sync -D --no-history
fi

cd src

# Apply patches only if not already applied (check with --reverse --check)
apply_patch_if_needed() {
  local patch="$1"
  if git apply --reverse --check "$patch" 2>/dev/null; then
    echo "Patch already applied: $(basename "$patch")"
  else
    echo "Applying patch: $(basename "$patch")"
    git apply "$patch" -v --ignore-space-change --ignore-whitespace --whitespace=nowarn || true
  fi
}

apply_patch_if_needed "$COMMAND_DIR/patches/add_licenses.patch"
apply_patch_if_needed "$COMMAND_DIR/patches/fix_license_json_parsing.patch"
apply_patch_if_needed "$COMMAND_DIR/patches/ssl_verify_callback_with_native_handle.patch"
apply_patch_if_needed "$COMMAND_DIR/patches/add_deps.patch"
apply_patch_if_needed "$COMMAND_DIR/patches/external_audio_source.patch"

cd ..

mkdir -p "$ARTIFACTS_DIR/lib"

debug="false"
if [ "$profile" = "debug" ]; then
  debug="true"
fi

# generate ninja files
# Note: use_clang_modules=false is required to avoid libc++ header incompatibility
# with Xcode 26.0. When enabled, C++ module compilation fails with errors like
# "unknown type name 'size_t'" due to conflicts between WebRTC's bundled libc++
# headers and the iOS SDK headers.
gn gen "$OUTPUT_DIR" --root="src" \
  --args="is_debug=$debug \
  enable_dsyms=$debug \
  target_os=\"ios\" \
  target_cpu=\"$arch\" \
  target_environment=\"$environment\" \
  treat_warnings_as_errors=false \
  ios_enable_code_signing=false \
  rtc_enable_protobuf=false \
  rtc_include_tests=false \
  rtc_build_examples=false \
  rtc_build_tools=false \
  rtc_libvpx_build_vp9=false \
  is_component_build=false \
  enable_stripping=true \
  rtc_enable_symbol_export=true \
  rtc_enable_objc_symbol_export=false \
  rtc_use_h264=false \
  use_custom_libcxx=false \
  use_clang_modules=false \
  clang_use_chrome_plugins=false \
  use_rtti=true \
  use_lld=false"

# build static library
ninja -C "$OUTPUT_DIR" :default \
  api/audio_codecs:builtin_audio_decoder_factory \
  api/task_queue:default_task_queue_factory \
  sdk:native_api \
  sdk:default_codec_factory_objc \
  pc:peer_connection \
  sdk:videocapture_objc \
  sdk:framework_objc

# make libwebrtc.a
# don't include nasm
ar -rc "$ARTIFACTS_DIR/lib/libwebrtc.a" `find "$OUTPUT_DIR/obj" -name '*.o' -not -path "*/third_party/nasm/*"`

# License generation - may fail locally due to GN warnings breaking JSON parsing
# Use vpython3 from depot_tools for consistent Python version
vpython3 "./src/tools_webrtc/libs/generate_licenses.py" \
  --target :webrtc "$OUTPUT_DIR" "$OUTPUT_DIR" || echo "Warning: License generation failed"

cp "$OUTPUT_DIR/obj/webrtc.ninja" "$ARTIFACTS_DIR"
cp "$OUTPUT_DIR/obj/modules/desktop_capture/desktop_capture.ninja" "$ARTIFACTS_DIR" 2>/dev/null || true
cp "$OUTPUT_DIR/args.gn" "$ARTIFACTS_DIR"

cp "$OUTPUT_DIR/LICENSE.md" "$ARTIFACTS_DIR"

cd src
find . -name "*.h" -print | cpio -pd "$ARTIFACTS_DIR/include"
find . -name "*.inc" -print | cpio -pd "$ARTIFACTS_DIR/include"
