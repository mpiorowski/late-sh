settings.log.stdout := true
settings.server.telnet.bind_addr := "0.0.0.0"
server.telnet(port=1234)

# Rewrite metadata to include duration in the title
# Format: Artist - Title | Duration
def rewrite_np_metadata(m) =
  title = m["title"]
  artist = m["artist"]
  dur = m["duration"]
  
  # Default to "Unknown" if missing
  artist = if artist == "" then "Unknown" else artist end
  
  # Only append duration if it exists
  new_title = if dur != "" then
    "#{artist} - #{title} | #{dur}"
  else
    "#{artist} - #{title}"
  end
  
  [("title", new_title), ("artist", "")]
end

last_lofi_uri = ref("")
last_classic_uri = ref("")
last_ambient_uri = ref("")
last_jazz_uri = ref("")

def avoid_repeat_lofi(r) =
  uri = request.uri(r)
  if uri == last_lofi_uri() then
    false
  else
    last_lofi_uri := uri
    true
  end
end

def avoid_repeat_classic(r) =
  uri = request.uri(r)
  if uri == last_classic_uri() then
    false
  else
    last_classic_uri := uri
    true
  end
end

def avoid_repeat_ambient(r) =
  uri = request.uri(r)
  if uri == last_ambient_uri() then
    false
  else
    last_ambient_uri := uri
    true
  end
end

def avoid_repeat_jazz(r) =
  uri = request.uri(r)
  if uri == last_jazz_uri() then
    false
  else
    last_jazz_uri := uri
    true
  end
end

# No live upstream radio streams remain.
# lofi: replaced with local CC0 playlist (HoliznaCC0)
# ambient: replaced with local curated FMA playlist (CC-BY 4.0)
# classic: replaced with local curated Musopen playlist (Public Domain)
# jazz: local playlist only for now

# Local playlists (fallback when streams are down)
lofi_local = playlist(
  mode="randomize",
  loop=true,
  check_next=fun (r) -> avoid_repeat_lofi(r),
  "/etc/liquidsoap/lofi.m3u"
)
classic_local = playlist(
  mode="randomize",
  loop=true,
  check_next=fun (r) -> avoid_repeat_classic(r),
  "/etc/liquidsoap/classic.m3u"
)
ambient_local = playlist(
  mode="randomize",
  loop=true,
  check_next=fun (r) -> avoid_repeat_ambient(r),
  "/etc/liquidsoap/ambient.m3u"
)
jazz_local = playlist(
  mode="randomize",
  loop=true,
  check_next=fun (r) -> avoid_repeat_jazz(r),
  "/etc/liquidsoap/jazz.m3u"
)

# Apply metadata rewriting to local playlists
lofi_local = metadata.map(rewrite_np_metadata, lofi_local)
classic_local = metadata.map(rewrite_np_metadata, classic_local)
ambient_local = metadata.map(rewrite_np_metadata, ambient_local)
jazz_local = metadata.map(rewrite_np_metadata, jazz_local)

# Lofi: local CC0 playlist only (HoliznaCC0)
lofi_safe = mksafe(lofi_local)
# Classic: local curated Musopen playlist only
classic_safe = mksafe(classic_local)
# Ambient: local curated FMA playlist only
ambient_safe = mksafe(ambient_local)
# Jazz: local playlist only
jazz_safe = mksafe(jazz_local)

vibe = ref("lofi")

def set_vibe(v) =
  if v == "lofi" or v == "classic" or v == "ambient" or v == "jazz" then
    vibe := v
    "ok"
  else
    "unknown vibe"
  end
end

def toggle_vibe() =
  if vibe() == "lofi" then
    vibe := "classic"
  else
    if vibe() == "classic" then
      vibe := "ambient"
    else
      if vibe() == "ambient" then
        vibe := "jazz"
      else
        vibe := "lofi"
      end
    end
  end
  "ok"
end

server.register(namespace="vibe", "set", fun (v) -> set_vibe(v))
server.register(namespace="vibe", "toggle", fun (_) -> toggle_vibe())

radio = switch(track_sensitive=false, [
  ({vibe() == "lofi"}, lofi_safe),
  ({vibe() == "classic"}, classic_safe),
  ({vibe() == "ambient"}, ambient_safe),
  ({vibe() == "jazz"}, jazz_safe),
  ({true}, lofi_safe),
])

output.icecast(
  %mp3(bitrate=128, samplerate=44100, stereo=true),
  host="icecast-sv",
  port=8000,
  password="hackme",
  mount="/stream",
  name="late.sh radio",
  description="late.sh vibe radio",
  radio
)
