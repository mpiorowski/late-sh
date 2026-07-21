use super::parse_feed;

#[test]
fn parse_feed_reads_rss_items() {
    let xml = r#"
        <rss><channel><title>Blog</title>
        <item><title>Hello</title><link>/hello</link><guid>1</guid><description><![CDATA[<p>Hi</p>]]></description></item>
        </channel></rss>
    "#;
    let feed = parse_feed("https://example.com/feed.xml", xml).expect("feed");
    assert_eq!(feed.title, "Blog");
    assert_eq!(feed.entries[0].url, "https://example.com/hello");
    assert_eq!(feed.entries[0].summary, "Hi");
}

#[test]
fn parse_feed_strips_entity_encoded_html() {
    let xml = r#"
        <rss><channel><title>Blog</title>
        <item><title>T</title><link>/x</link><guid>1</guid><description>&lt;table border=0&gt;&lt;tr&gt;&lt;td&gt;Hello world&lt;/td&gt;&lt;/tr&gt;&lt;/table&gt;</description></item>
        </channel></rss>
    "#;
    let feed = parse_feed("https://example.com/feed.xml", xml).expect("feed");
    assert_eq!(feed.entries[0].summary, "Hello world");
}

#[test]
fn parse_feed_reads_atom_entries() {
    let xml = r#"
        <feed><title>Atom</title>
        <entry><title>Post</title><id>tag:post</id><link href="https://example.com/post" /></entry>
        </feed>
    "#;
    let feed = parse_feed("https://example.com/feed", xml).expect("feed");
    assert_eq!(feed.entries[0].url, "https://example.com/post");
}
