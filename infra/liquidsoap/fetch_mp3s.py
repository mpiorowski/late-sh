import urllib.request, json
def get_mp3s(query, max_items=5):
    url = f"https://archive.org/advancedsearch.php?q=subject%3A%22{query}%22+AND+mediatype%3A%22audio%22&fl%5B%5D=identifier&sort%5B%5D=downloads+desc&rows={max_items}&page=1&output=json"
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    res = urllib.request.urlopen(req).read()
    data = json.loads(res)
    urls = []
    for doc in data["response"]["docs"]:
        ident = doc["identifier"]
        meta_url = f"https://archive.org/metadata/{ident}"
        req_m = urllib.request.Request(meta_url, headers={"User-Agent": "Mozilla/5.0"})
        res_m = urllib.request.urlopen(req_m).read()
        meta = json.loads(res_m)
        server = meta.get("server", "archive.org")
        dir_ = meta.get("dir", "")
        for f in meta.get("files", []):
            if f.get("format") in ("VBR MP3", "MP3", "128Kbps MP3") and f.get("name", "").endswith(".mp3"):
                title = f.get("title", f.get("name")).replace('"', "'")
                creator = meta.get("metadata", {}).get("creator", ["Unknown Artist"])
                if isinstance(creator, list):
                    artist = creator[0].replace('"', "'")
                else:
                    artist = str(creator).replace('"', "'")
                
                url = f"https://{server}{dir_}/{f['name']}"
                urls.append(f'annotate:artist="{artist}",title="{title}":{url}')
                if len(urls) >= 5: return urls
    return urls

print("Lofi:")
for u in get_mp3s("lofi"): print(u)
print("Ambient:")
for u in get_mp3s("ambient"): print(u)
print("Classic:")
for u in get_mp3s("classical"): print(u)
